use crate::alloc::*;
use crate::ll::*;
use crate::utils::*;
use std::ops::{Index,IndexMut};
use std::marker::PhantomData;
use std::mem;

#[repr(transparent)]
#[derive(Clone, Debug)]
/// Buddy memory block
/// Each memory block has some meta-data information in form of `Buddy` data
/// structure. It has a pointer to the next buddy block, if there is any.
struct Buddy {
    /// Next pointer offset
    /// We assume that usize::MAX is NULL
    next: u64,
}

impl Default for Buddy {
    fn default() -> Self {
        Self { next: u64::MAX }
    }
}

#[inline]
fn is_none(p: u64) -> bool {
    p == u64::MAX
}

#[inline]
fn off_to_option(p: u64) -> Option<u64> {
    if is_none(p) {
        None
    } else {
        Some(p)
    }
}

#[inline]
fn option_to_pptr(p: Option<u64>) -> u64 {
    if let Some(p) = p {
        p
    } else {
        u64::MAX
    }
}

#[repr(C)]
/// Buddy Allocation Algorithm
///
/// It contains 61 free-lists of available buddy blocks to keep at most `2^64`
/// bytes including meta-data information. A free-list `k` keeps all available
/// memory blocks of size `2^k` bytes. Assuming that `Buddy` has a size of 
/// 8 bytes, the shape of lists can be like this:
///
/// ```text
///    [8]: [8] -> [8]
///   [16]: [8|8] -> [8|8]
///   [32]: [8|24] -> [8|24] -> [8|24]
///   [64]: [8|56]
///   ...
/// ```
///
/// The first 8 bytes of each free block is meta-data. Once they are selected
/// for occupation, this 8 byte is going to be used, too. So, the smallest block
/// size is 8 bytes.
pub struct BuddyAlg<A: MemPool> {
    /// Lists of free blocks
    buddies: [u64; 64],

    /// The index of the last buddy list
    last_idx: usize,

    /// Total available space in bytes
    available: usize,

    /// The device size in bytes
    size: usize,

    /// An axillary ring list for allocation and recovery
    aux: Ring<(u64, u64), 128>,

    /// Low-level 64-bit logs for allocation and recovery
    log64: Ring<(u64, u64), 8>,

    /// Low-level `DropOnFailure` logs for recovery
    drop_log: Ring<(u64, usize), 8>,

    /// Indicates that it is draining `aux`
    aux_valid: bool,

    /// Log of available space
    available_log: usize,

    #[cfg(feature = "stat_footprint")]
    /// The stat_footprint of memory usage in bytes
    foot_print: usize,

    #[cfg(not(any(feature = "no_pthread", windows)))]
    /// A mutex for atomic operations
    mutex: (libc::pthread_mutex_t, libc::pthread_mutexattr_t),

    #[cfg(any(feature = "no_pthread", windows))]
    /// A mutex for atomic operations
    mutex: u64,

    // Marker
    phantom: PhantomData<A>,
}

#[inline]
const fn num_bits<T>() -> u32 {
    (mem::size_of::<T>() << 3) as u32
}

#[inline]
pub fn get_idx(x: usize) -> usize {
    if x == 0 {
        usize::MAX
    } else {
        let x = x.max(mem::size_of::<Buddy>());
        (num_bits::<usize>() - (x - 1).leading_zeros()) as usize
    }
}

impl<A: MemPool> BuddyAlg<A> {
    /// Pool Initialization with a given device size
    pub fn init(&mut self, base: u64, size: usize) {
        let mut idx = get_idx(size);
        if 1 << idx > size {
            idx -= 1;
        }
        self.buddies = [u64::MAX; 64];
        self.size = 1 << idx;
        self.available = self.size;
        self.buddies[idx] = base;
        self.last_idx = idx;
        self.log64.clear();
        self.drop_log.clear();
        self.aux.clear();

        Self::buddy(base).next = u64::MAX;

        #[cfg(not(any(feature = "no_pthread", windows)))] unsafe {
        crate::sync::init_lock(&mut self.mutex.0, &mut self.mutex.1);
        }

        #[cfg(any(feature = "no_pthread", windows))] {
        self.mutex = 0; }
    }

    #[inline]
    fn in_range<'a>(off: u64) -> bool {
        (off < u64::MAX - A::start()) && (off + A::start() < A::end())
    }

    #[inline]
    #[track_caller]
    fn buddy<'a>(off: u64) -> &'a mut Buddy {
        debug_assert!(Self::in_range(off), "off(0x{:x}) out of range", off);
        unsafe { read_addr(A::start() + off) }
    }

    #[inline]
    fn byte<'a>(off: u64) -> &'a mut u8 {
        unsafe { read_addr(A::start() + off) }
    }

    #[inline]
    fn lock(&mut self) {
        unsafe { 
            // debug_assert!(self.aux.empty(), "locked before: aux is not empty");

            #[cfg(not(any(feature = "no_pthread", windows)))]
            libc::pthread_mutex_lock(&mut self.mutex.0); 

            #[cfg(any(feature = "no_pthread", windows))] {
                let tid = std::thread::current().id().as_u64().get();
                while std::intrinsics::atomic_cxchg_acqrel(&mut self.mutex, 0, tid).0 != tid {}
            }
        }
    }

    #[inline]
    fn unlock(&mut self) {
        unsafe { 
            #[cfg(not(any(feature = "no_pthread", windows)))]
            libc::pthread_mutex_unlock(&mut self.mutex.0); 

            #[cfg(any(feature = "no_pthread", windows))]
            std::intrinsics::atomic_store_rel(&mut self.mutex, 0);
        }
    }

    #[inline]
    /// Adds a new low-level 64-bit log entry
    pub unsafe fn log(&mut self, off: u64, data: u64) {
        self.log64.push((off, data));
    }

    #[inline]
    /// Adds a new low-level `DropOnFailure` log entry
    pub unsafe fn drop_on_failure(&mut self, off: u64, len: usize) {
        self.drop_log.push((off, len));
    }

    #[inline]
    /// Adds a new entry to the auxiliary list of changes
    pub unsafe fn aux_push(&mut self, off: u64, data: u64) {
        self.aux.push((off, data));
    }

    #[inline]
    /// Drain the auxiliary list of changes
    /// 
    /// The functions [`alloc_impl`] and [`dealloc_impl`] fills up the auxiliary
    /// buffer with the required changes to the free lists. Then, they call this
    /// function to materialize the changes. The changes are not valid until
    /// `drain_aux()` is called. The recovery procedure performs changes if they
    /// are valid. Otherwise, it discards them.
    /// 
    /// [`alloc_impl`]: #method.alloc_impl
    /// [`dealloc_impl`]: #method.dealloc_impl
    pub fn drain_aux(&mut self) {
        sfence();

        self.aux_valid = true;
        self.aux.foreach(|(off, next)| {
            let n = Self::buddy(off);
            n.next = next;
        });
        self.aux.clear();
        self.log64.foreach(|(off, data)| unsafe {
            let n = Self::buddy(off);
            std::intrinsics::atomic_store_rel(&mut n.next, data);
        });
        self.log64.clear();
        self.available = self.available_log;
    }

    #[inline(always)]
    /// Begins a failure-atomic section
    pub unsafe fn prepare(&mut self) {
        self.lock();
        self.log64.clear();
        self.aux_valid = true;
    }

    #[inline]
    /// Materializes the changes in the auxiliary list and clears the drop log
    /// records
    pub unsafe fn perform(&mut self) {
        self.drain_aux();
        self.drop_log.clear();
        self.aux_valid = false;
        self.unlock();
    }

    #[inline]
    /// Discards the changes in the auxiliary buffer
    pub fn discard(&mut self) {
        self.aux.clear();
        self.log64.clear();
        self.drop_log.clear();
        self.unlock();
    }

    #[inline]
    fn get_off(b: &u64) -> u64 {
        let off = b as *const _ as u64;
        off - A::start()
    }

    #[inline]
    unsafe fn find_free_memory(&mut self, idx: usize, split: bool) -> Option<u64> {
        if idx > self.last_idx {
            // TODO: Check if there are enough free adjacent blocks to add up
            //       to the requested size.
            None
        } else {
            let res;
            if let Some(b) = off_to_option(self.buddies[idx]) {
                // Remove the available block and return it
                let buddy = Self::buddy(b);
                self.aux_push(Self::get_off(&self.buddies[idx]), buddy.next);
                res = b;
            } else {
                res = self.find_free_memory(idx + 1, true)?;
            }
            if idx > 0 && split {
                let next = res + (1 << (idx - 1));
                let mut curr = self.buddies[idx - 1];
                let mut prev: Option<u64> = None;

                while let Some(b) = off_to_option(curr) {
                    if b > next {
                        break;
                    }
                    prev = Some(b);
                    curr = Self::buddy(b).next;
                }

                if let Some(p) = prev {
                    self.aux_push(next, Self::buddy(p).next);
                    self.aux_push(p, next);
                } else {
                    self.aux_push(next, self.buddies[idx - 1]);
                    self.aux_push(Self::get_off(&self.buddies[idx - 1]), next);
                }
            }
            Some(res)
        }
    }

    #[inline]
    /// Generates required changes to the metadata for allocating a new memory
    /// block with the size `len`, and materialize them by calling
    /// [`drain_aux`](#methods.drain_aux) according to the `perform` argument.
    /// If successful, it returns the offset of the available free block.
    /// Otherwise, `u64::MAX` is returned.
    pub unsafe fn alloc_impl(&mut self, len: usize, perform: bool) -> u64 {
        self.lock();
        let idx = get_idx(len);
        let len = 1 << idx;

        if len > self.available {
            self.discard();
            u64::MAX
        } else {
            match self.find_free_memory(idx, false) {
                Some(off) => {
                    #[cfg(feature = "verbose")]
                    debug_alloc::<A>(off, len, self.used(), self.used() + (1 << idx));

                    self.available_log = self.available - len;

                    self.aux.sync_all();
                    if perform {
                        self.perform();
                    }

                    #[cfg(feature = "stat_footprint")]
                    {
                        let usage = self.size - self.available_log;
                        if usage > self.foot_print {
                            self.foot_print = usage;
                        }
                    }

                    off
                }
                None => {
                    eprintln!(
                        "Cannot find memory slot of size {} (available: {})",
                        len,
                        self.available()
                    );
                    self.discard();
                    u64::MAX
                }
            }
        }
    }

    #[inline]
    /// Generates required changes to the metadata for reclaiming the memory
    /// block at offset `off` with the size of `len`, and materialize them by
    /// calling [`drain_aux`](#methods.drain_aux) according to the `perform`
    /// argument.
    pub unsafe fn dealloc_impl(&mut self, off: u64, len: usize, perform: bool) {
        self.lock();
        let idx = get_idx(len);
        let len = 1 << idx;

        #[cfg(feature = "verbose")]
        debug_dealloc::<A>(off, len, self.used(), self.used() - len);

        self.available_log = self.available;
        self.free_impl(off, len);

        self.aux.sync_all();
        if perform {
            self.perform();
        }
    }

    #[inline]
    unsafe fn free_impl(&mut self, off: u64, len: usize) {
        let idx = get_idx(len);
        let end = off + (1 << idx);
        let mut curr = self.buddies[idx];
        let mut prev: Option<u64> = None;
        if idx < self.last_idx {
            while let Some(b) = off_to_option(curr) {
                let e = Self::buddy(b);
                let on_left = off & (1 << idx) == 0;
                if (b == end && on_left) || (b + len as u64 == off && !on_left) {
                    let off = off.min(b);
                    if let Some(p) = prev {
                        self.aux_push(p, e.next);
                    } else {
                        self.aux_push(Self::get_off(&self.buddies[idx]), e.next);
                    }
                    self.available_log -= len;
                    self.free_impl(off, len << 1);
                    return;
                }
                if b > off {
                    break;
                }
                prev = Some(b);
                curr = e.next;
                // if curr == b {
                //     eprintln!("Double free for @{} ({})", off, len);
                //     self.aux.clear();
                //     return;
                // }
                debug_assert_ne!(curr, b, "Cyclic link in free_impl");
            }
        }
        if let Some(p) = prev {
            self.aux_push(off, Self::buddy(p).next);
            self.aux_push(p, off);
        } else {
            self.aux_push(off, self.buddies[idx]);
            self.aux_push(Self::get_off(&self.buddies[idx]), off);
        }
        self.available_log += len;
    }

    #[inline]
    /// Determines if the given address range is allocated
    pub fn is_allocated(&mut self, off: u64, _len: usize) -> bool {
        self.lock();

        if !self.aux.is_empty() {
            // self.discard();
            return true;
        }

        let end = off + _len as u64 - 1;
        let idx = get_idx(_len);
        for idx in idx..self.last_idx + 1 {
            let len = 1 << idx;
            let mut curr = self.buddies[idx];

            #[cfg(feature = "check_allocator_cyclic_links")]
            let mut links = vec![];

            while let Some(b) = off_to_option(curr) {
                #[cfg(feature = "check_allocator_cyclic_links")]
                {
                    if links.contains(&b) {
                        self.discard();
                        panic!("A cyclic link detected in list {}", idx);
                    } else {
                        links.push(b);
                    }
                }

                let r = b + len;
                if (off >= b && off < r) || (end >= b && end < r) || (off <= b && end >= r) {
                    self.discard();
                    return false;
                }
                if b > off {
                    break;
                }
                curr = Self::buddy(b).next;
                debug_assert_ne!(curr, b, "Cyclic link in is_allocated");
            }
        }
        self.discard();
        true
    }

    #[inline]
    /// Starts the recovery procedure. If the crash happened while draining the
    /// auxiliary buffer, it continues draining it and making the remaining
    /// changes. It is rational because the [`DropOnFailure`] log was taken
    /// before draining the auxiliary buffer. When the draining is finished,
    /// the higher-level log reclaims the allocation in the higher level
    /// recovery procedure.
    /// 
    /// [`DropOnFailure`]: ../alloc/trait.MemPool.html#method.drop_on_failure
    pub fn recover(&mut self) {
        #[cfg(not(any(feature = "no_pthread", windows)))] unsafe {
        crate::sync::init_lock(&mut self.mutex.0, &mut self.mutex.1);
        }

        #[cfg(any(feature = "no_pthread", windows))] {
        self.mutex = 0; }


        #[cfg(feature = "check_allocator_cyclic_links")]
        if !self.verify() {
            eprintln!("not verified before recovery");
        }

        if self.aux_valid {
            #[cfg(debug_assertions)]
            eprintln!("Crashed while the allocator was operating");

            #[cfg(feature = "verbose")] {
                if *crate::utils::VERBOSE {
                    self.aux.foreach(|(off, next)| {
                        let n = Self::buddy(off);
                        println!("aux @({:x}) {:x} -> {:x}", off, n.next, next);
                    });
    
                    self.log64.foreach(|(off, next)| {
                        let n = Self::buddy(off);
                        println!("log @({:x}) {:x} -> {:x}", off, n.next, next);
                    });
    
                    self.drop_log.foreach(|(off, len)| {
                        println!("drop ({:x}; {})", off, len);
                    });
                }
            }

            // continue draining
            self.drain_aux();

            // drop unnecessary allocations
            if !self.drop_log.is_empty() {
                eprintln!("Dropping unnecessary allocations");
                unsafe {
                    let self_mut = self as *mut Self;
                    self.drop_log.drain_atomic(|(off, len)| {
                        (*self_mut).dealloc_impl(off, len, false);
                    }, || {
                        (*self_mut).drain_aux();
                        (*self_mut).discard();
                    });
                }
                self.drop_log.clear();
            }

            #[cfg(debug_assertions)]
            self.check(module_path!());
        } else {
            self.aux.clear();
            self.log64.clear();
            self.drop_log.clear();
        }

        #[cfg(feature = "check_allocator_cyclic_links")]
        if !self.verify() {
            eprintln!("not verified after recovery");
        }
    }

    pub fn recovery_info(&self, info_level: u32) -> String {
        let mut res = format!("Crashed while operating: {}\n",
            if self.aux_valid { "Yes" } else { "No" });
        if info_level > 1 {
            res += &format!("Redo Operation Logs (aux): {}\n", self.aux.len());
            res += &format!("Redo Logs (log64):         {}\n", self.log64.len());
            res += &format!("Drop Logs:                 {}\n", self.drop_log.len());
        }
        if info_level > 2 {
            if !self.aux.is_empty() {
                res += &format!("\nOperation Logs:\n");
                self.aux.foreach(|(off, next)| {
                    let n = Self::buddy(off);
                    res += &format!("  aux @({:x}) {:x} -> {:x}\n", off, n.next, next);
                });
            }
    
            if !self.log64.is_empty() {
                res += &format!("\nRedo Logs:\n");
                self.log64.foreach(|(off, next)| {
                    let n = Self::buddy(off);
                    res += &format!("  log @({:x}) {:x} -> {:x}\n", off, n.next, next);
                });
            }
    
            if !self.drop_log.is_empty() {
                res += &format!("\nDrop Logss:\n");
                self.drop_log.foreach(|(off, len)| {
                    res += &format!("  drop ({:x}; {})\n", off, len);
                });
            }
        }
        res
    }

    #[inline]
    /// Returns the pool size
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    /// Returns the total available space in the pool
    pub fn available(&self) -> usize {
        self.available
    }

    #[inline]
    /// Returns the total number of bytes used from the pool
    pub fn used(&self) -> usize {
        self.size - self.available
    }

    #[cfg(feature = "stat_footprint")]
    /// Returns the total number of bytes written to the pool. It may exceed the
    /// pool size as it does not subtract the reclaimed space after being used.
    pub fn stat_footprint(&self) -> usize {
        self.foot_print
    }

    pub fn check(&self, f: &str) {
        for idx in 3..self.last_idx + 1 {
            let mut curr = self.buddies[idx];
            while let Some(b) = off_to_option(curr) {
                let e = Self::buddy(b);
                curr = e.next;
                assert_ne!(curr, b, "Cyclic link in checking {}", f);
            }
        }
    }

    pub fn verify(&mut self) -> bool {
        if std::env::var("VERIFY").is_err() { return true; }
        let loops = std::env::var("VERIFY").unwrap() == "2";
        self.lock();
        for idx in 3..self.last_idx + 1 {
            let mut curr = self.buddies[idx];
            let mut links = vec![];
            while let Some(b) = off_to_option(curr) {
                if { if loops { links.contains(&b) } else { false } } || !Self::in_range(b) {
                    self.unlock();
                    if !Self::in_range(b) {
                        eprintln!("Verification Failed: Invalid block address 0x{:x} (idx={})", b, idx);
                    } else {
                        eprintln!("Verification Failed: A cyclic link detected in list {}", idx);
                    }
                    return false;
                }
                if loops { links.push(b); }
                let e = Self::buddy(b);
                curr = e.next;
            }
        }
        self.unlock(); 
        true
    }

    /// Prints the free lists
    pub fn print(&self) {
        println!();
        for idx in 3..self.last_idx + 1 {
            print!("{:>12} [{:>2}] ", 1 << idx, idx);
            let mut curr = self.buddies[idx];
            while let Some(b) = off_to_option(curr) {
                let e = Self::buddy(b);
                if A::contains(b+A::start()) {
                    print!("({}:{})", b, b + (1 << idx) - 1);
                } else {
                    print!("(ERR)");
                    break;
                }
                curr = e.next;
            }
            println!();
        }
    }
}

/// Memory Zones
/// 
/// It manages memory zones to optimally dedicate a zone to each cpu for 
/// scalability.
pub struct Zones<T, A: MemPool> {
    count: usize,
    quota: usize,
    base: usize,
    phantom: PhantomData<(T,A)>
}

impl<T, A: MemPool> Zones<T, A> {

    /// Creates a new `Zones` object
    /// 
    /// * `count` is the number of zones (usually is the number cpus)
    /// * `offset` is the size of reserved memory to be allocated for metadata
    /// 
    pub fn new(count: usize, offset: usize, quota: usize) -> Self {
        assert!(offset <= quota, "Memory quota exceeds the reserved memory ({} > {})", offset, quota);
        Self { count, quota, base: offset, phantom: PhantomData }
    }

    #[inline]
    /// Returns the size of zones
    pub fn quota(&self) -> usize {
        self.quota
    }

    #[inline]
    /// Returns the number of zones
    pub fn count(&self) -> usize {
        self.count
    }

    #[inline]
    /// Returns a mutable reference to the i-th zone object
    pub fn at(&self, i: usize) -> &mut T {
        debug_assert!(i < self.count, "no zone index {} (max = {})", i, self.count);
        let off = self.base + i * mem::size_of::<T>();
        Self::read(off as u64)
    }

    #[inline]
    /// Returns a mutable reference to the zone object associated with the
    /// current cpu
    pub fn get(&self) -> &mut T {
        let i = cpu() % self.count;
        let off = self.base + i * mem::size_of::<T>();
        Self::read(off as u64)
    }

    #[inline]
    /// Returns a mutable reference to the zone object associated with the
    /// given offset
    pub fn from_off(&self, off: u64) -> (&mut T, usize) {
        let i = off as usize / self.quota;
        let off = self.base + i * mem::size_of::<T>();
        (Self::read(off as u64), i)
    }

    #[inline]
    fn read<'a>(off: u64) -> &'a mut T {
        unsafe { read_addr(A::start() + off) }
    }
}

impl<T, A: MemPool> Index<usize> for Zones<T, A> {
    type Output = T;
    fn index(&self, i: usize) -> &T { self.at(i) }
}

impl<T, A: MemPool> IndexMut<usize> for Zones<T, A> {
    fn index_mut(&mut self, i: usize) -> &mut T { self.at(i) }
}

#[cfg(test)]
mod test {
    use crate::RootObj;
    use crate::default::*;
    use crate::open_flags::*;
    type P = Allocator;

    #[test]
    fn buddy_alg_test() {
        use rand::distributions::Alphanumeric;
        use rand::Rng;

        struct Root {
            vec: PRefCell<PVec<Parc<(i32, PMutex<PString>)>>>
        }
        impl RootObj<P> for Root {
            fn init(_: &Journal) -> Self {
                Root {
                    vec: PRefCell::new(PVec::new())
                }
            }
        }
        let root = P::open::<Root>("buddy.pool", O_CFNE).unwrap();
        let u = P::used();
        P::transaction(|j| {
            let _b = Pbox::new(1, j);
            let _b = Pbox::new([0;8], j);
            let _b = Pbox::new([0;64], j);
            let _b = Pbox::new([0;1024], j);
            let _b = Pbox::new([0;4096], j);
            let _b = Pbox::new([0;10000], j);
        }).unwrap();

        P::transaction(|j| {
            let _b = Pbox::new([0;10000], j);
            let _b = Pbox::new([0;8], j);
            let _b = Pbox::new([0;1024], j);
            let _b = Pbox::new([0;64], j);
            let _b = Pbox::new(1, j);
            let _b = Pbox::new([0;4096], j);
        }).unwrap();

        P::transaction(|j| {
            let mut b = root.vec.borrow_mut(j);
            for i in 0..2 {
                b.push(Parc::new((i, PMutex::new(format!("item {}", i).to_pstring(j))), j), j);
            }
        }).unwrap();

        let mut ts = vec![];
        for i in 0..2 {
            let m = root.vec.borrow()[i].demote();
            ts.push(std::thread::spawn(move || {
                P::transaction(|j| {
                    if let Some(m) = m.promote(j) {
                        let mut m = m.1.lock(j);
                        let l = (rand::random::<usize>() % 100) + 1;
                        let s: Vec<u8> = //String::from_utf8(
                            rand::thread_rng()
                                .sample_iter(&Alphanumeric)
                                .take(l)
                                .collect();
                            //).unwrap();
                        *m = crate::str::String::from_utf8(s, j).unwrap();
                    }
                }).unwrap();
            }));
        }

        for t in ts {
            t.join().unwrap();
        }

        P::transaction(|j| {
            let mut vec = root.vec.borrow_mut(j);
            if vec.len() > 10 {
                vec.clear();
            }
        }).unwrap();

        println!("{} -> {}", u, P::used());
    }
}

#[cfg(feature = "verbose")]
#[macro_export]
macro_rules! __cfg_verbose {
    ($blk:block) => { #[allow(unused_braces)] $blk };
    ($if:block,$else:block) => { #[allow(unused_braces)] $if };
}

#[cfg(not(feature = "verbose"))]
#[macro_export]
macro_rules! __cfg_verbose {
    ($blk:block) => { };
    ($if:block,$else:block) => { #[allow(unused_braces)] $else };
}

#[cfg(feature = "check_access_violation")]
#[macro_export]
macro_rules! __cfg_check_access_violation {
    ($blk:block) => { #[allow(unused_braces)] $blk };
    ($if:block,$else:block) => { #[allow(unused_braces)] $if };
}

#[cfg(not(feature = "check_access_violation"))]
#[macro_export]
macro_rules! __cfg_check_access_violation {
    ($blk:block) => { };
    ($if:block,$else:block) => { #[allow(unused_braces)] $else }
}

#[cfg(feature = "pin_journals")]
#[macro_export]
macro_rules! __cfg_pin_journals {
    ($blk:block) => { #[allow(unused_braces)] $blk };
    ($if:block,$else:block) => { #[allow(unused_braces)] $if };
}

#[cfg(not(feature = "pin_journals"))]
#[macro_export]
macro_rules! __cfg_pin_journals {
    ($blk:block) => { };
    ($if:block,$else:block) => { #[allow(unused_braces)] $else };
}

#[cfg(feature = "check_allocator_cyclic_links")]
#[macro_export]
macro_rules! __cfg_check_allocator_cyclic_links {
    ($blk:block) => { #[allow(unused_braces)] $blk };
    ($if:block,$else:block) => { #[allow(unused_braces)] $if };
}

#[cfg(not(feature = "check_allocator_cyclic_links"))]
#[macro_export]
macro_rules! __cfg_check_allocator_cyclic_links {
    ($blk:block) => { };
    ($if:block,$else:block) => { #[allow(unused_braces)] $else };
}

#[cfg(feature = "stat_perf")]
#[macro_export]
macro_rules! __cfg_stat_perf {
    ($blk:expr) => { #[allow(unused_braces)] $blk };
    ($if:expr,$else:expr) => { #[allow(unused_braces)] $if }
}

#[cfg(not(feature = "stat_perf"))]
#[macro_export]
macro_rules! __cfg_stat_perf {
    ($blk:expr) => { () };
    ($if:expr,$else:expr) => { #[allow(unused_braces)] $else }
}

#[cfg(feature = "stat_footprint")]
#[macro_export]
macro_rules! __cfg_stat_footprint {
    ($blk:block) => { #[allow(unused_braces)] $blk };
    ($if:block,$else:block) => { #[allow(unused_braces)] $if };
}

#[cfg(not(feature = "stat_footprint"))]
#[macro_export]
macro_rules! __cfg_stat_footprint {
    ($blk:block) => { };
    ($if:block,$else:block) =>  { #[allow(unused_braces)] $else }
}

#[cfg(feature = "check_double_free")]
#[macro_export]
macro_rules! __cfg_delete_history {
    ($blk:block) => { $blk };
    ($if:block,$else:block) => { $if };
}

#[cfg(not(feature = "check_double_free"))]
#[macro_export]
macro_rules! __cfg_delete_history {
    ($blk:block) => { };
    ($if:block,$else:block) =>  { $else }
}


#[macro_export]
/// This macro creates a new pool module and aliases for persistent types. It
/// generates type [`Allocator`] which a persistent allocator type. It is
/// recommended to alias the [`Allocator`] type for tidiness.
/// 
/// The aliased types are 
/// 
/// * `Pbox<T>` = [`corundum::boxed::Pbox`]`<T, `[`Allocator`]`>`
/// * `Prc<T>` = [`corundum::prc::Prc`]`<T, `[`Allocator`]`>`
/// * `Parc<T>` = [`corundum::sync::Parc`]`<T, `[`Allocator`]`>`
/// * `PMutex<T>` = [`corundum::sync::PMutex`]`<T, `[`Allocator`]`>`
/// * `PCell<T>` = [`corundum::cell::PCell`]`<T, `[`Allocator`]`>`
/// * `PRefCell<T>` = [`corundum::cell::PRefCell`]`<T, `[`Allocator`]`>`
/// * `VCell<T>` = [`corundum::cell::VCell`]`<T, `[`Allocator`]`>`
/// * `TCell<T>` = [`corundum::cell::TCell`]`<T, `[`Allocator`]`>`
/// * `PVec<T>` = [`corundum::vec::Vec`]`<T, `[`Allocator`]`>`
/// * `PString` = [`corundum::str::String`]`<`[`Allocator`]`>`
///
/// # Examples
/// 
/// To associate a single pool to the program, it is enough to define a pool
/// type using this macro.
/// 
/// ```
/// # fn main() {
/// corundum::pool!(my_alloc);
/// use my_alloc::*;
/// 
/// type P = Allocator;
/// 
/// let _pool = P::open_no_root("p.pool", O_CF).unwrap();
/// 
/// P::transaction(|j| {
///     let temp = Pbox::new(10, j);
/// }).unwrap();
/// # }
/// ```
/// 
/// If multiple pools are needed, multiple pool modules can be defined and used.
/// 
/// ```
/// use corundum::alloc::heap::*;
/// 
/// corundum::pool!(pool1);
/// corundum::pool!(pool2);
/// 
/// type P1 = pool1::Allocator;
/// type P2 = pool2::Allocator;
/// 
/// let _p1 = P1::open_no_root("p1.pool", O_CF).unwrap();
/// let _p2 = P2::open_no_root("p2.pool", O_CF).unwrap();
/// 
/// P1::transaction(|j1| {
///     let temp = pool1::Pbox::new(10, j1);
///     P2::transaction(|j2| {
///         let temp = pool2::Pbox::new(20, j2);
///     }).unwrap();
/// }).unwrap();
/// ```
/// 
/// [`Allocator`]: ./alloc/default/struct.Allocator.html
/// [`corundum::boxed::Pbox`]: ./boxed/struct.Pbox.html
/// [`corundum::prc::Prc`]: ./prc/struct.Prc.html
/// [`corundum::sync::Parc`]: ./sync/struct.Parc.html
/// [`corundum::sync::PMutex`]: ./sync/struct.PMutex.html
/// [`corundum::cell::PCell`]: ./cell/struct.PCell.html
/// [`corundum::cell::PRefCell`]: ./cell/struct.PRefCell.html
/// [`corundum::cell::VCell`]: ./cell/struct.VCell.html
/// [`corundum::cell::TCell`]: ./cell/struct.TCell.html
/// [`corundum::vec::Vec`]: ./vec/struct.Vec.html
/// [`corundum::str::String`]: ./str/struct.String.html
macro_rules! pool {
    ($mod:ident, $name:ident) => {
        /// The default allocator module
        pub mod $mod {
            use memmap::*;
            use std::collections::hash_map::DefaultHasher;
            use std::collections::{HashMap,HashSet};
            use std::fs::OpenOptions;
            use std::hash::{Hash, Hasher};
            use std::mem;
            use std::ops::Range;
            use std::path::{Path, PathBuf};
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::{Arc, Mutex};
            use std::thread::ThreadId;
            use $crate::ll::*;
            use $crate::result::Result;
            use $crate::utils::read;
            use $crate::*;
            pub use $crate::{
                PSafe, 
                TxInSafe, 
                TxOutSafe, 
                LooseTxInUnsafe, 
                AssertTxInSafe, 
                VSafe, 
                transaction, 
                open_flags, 
                PClone, 
                Root,
                RootObj,
                ToPString,
                ToPStringSlice,
                MemPoolTraits,
                MemPool
            };
    
            static mut BUDDY_START: u64 = 0;
            static mut BUDDY_VALID_START: u64 = 0;
            static mut BUDDY_END: u64 = 0;
    
            #[repr(C)]
            struct BuddyAllocInner {
                magic_number: u64,
                flags: u64,
                gen: u32,
                tx_gen: u32,
                root_obj: u64,
                root_type_id: u64,
                journals: u64,
                size: usize,
                zone: Zones<BuddyAlg<$name>, $name>
            }
    
            struct VData {
                filename: String,
                journals: HashMap<ThreadId, (u64, i32)>,
                check_double_free: HashSet<u64>,
                mmap: MmapMut,
            }
    
            impl VData {
                fn new(mmap: MmapMut, filename: &str) -> Self {
                    Self {
                        filename: filename.to_string(),
                        journals: HashMap::new(),
                        check_double_free: HashSet::new(),
                        mmap,
                    }
                }
            }
    
            impl BuddyAllocInner {
                fn init(&mut self, size: usize) {
                    let id = std::any::type_name::<Self>();
                    let mut s = DefaultHasher::new();
                    id.hash(&mut s);
                    self.flags = 0;
                    self.gen = 1;
                    self.tx_gen = 0;
                    self.root_obj = u64::MAX;
                    self.root_type_id = 0;
                    self.journals = u64::MAX;
                    self.size = size;
    
                    type T = BuddyAlg<$name>;
                    let cpus = if let Some(val) = std::env::var_os("CPUS") {
                        val.into_string().unwrap().parse::<usize>().unwrap()
                    } else {
                        num_cpus::get()
                    };
                    assert_ne!(cpus, 0);
                    let quota = size / cpus;
                    self.zone = Zones::new(cpus, mem::size_of::<Self>(), quota);
                    for i in 0..cpus {
                        self.zone[i].init((quota * i) as u64, quota);
                    }
                    self.magic_number = u64::MAX;
                    unsafe {
                        self.zone[0].alloc_impl(
                            mem::size_of::<Self>() + mem::size_of::<T>() * cpus,
                            true,
                        );
                    }
                    self.magic_number = s.finish();
                }
    
                fn as_bytes(&self) -> &[u8] {
                    let ptr: *const Self = self;
                    let ptr = ptr as *const u8;
                    unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<Self>()) }
                }
    
                fn has_root(&self) -> bool {
                    self.flags & FLAG_HAS_ROOT == FLAG_HAS_ROOT
                }
            }
    
            /// A memory allocator with buddy allocation mechanism
            ///
            /// To define a new buddy allocator type as a memory pool, you may
            /// use [`pool!()`] macro. 
            /// 
            /// [`pool!()`]: ../macro.pool.html
            #[derive(Clone,Copy,Default)]
            pub struct $name {}

            unsafe impl MemPool for $name {}
    
            pub mod dummy {
                #[repr(C)]
                pub enum Option<T> {
                    Some(T), None
                }
            }
    
            static mut BUDDY_INNER: Option<*mut BuddyAllocInner> = None;
            static mut OPEN: AtomicBool = AtomicBool::new(false);
            static mut MAX_GEN: u32 = 0;
            static mut VDATA: LazyCell<Arc<Mutex<Option<VData>>>> = 
                LazyCell::new(|| Arc::new(Mutex::new(None)));
    
            impl $name {
                fn running_transaction() -> bool {
                    let vdata = match unsafe { VDATA.lock() } {
                        Ok(g) => g,
                        Err(p) => p.into_inner()
                    };
                    if let Some(vdata) = &*vdata {
                        !vdata.journals.is_empty()
                    } else {
                        false
                    }
                }
    
                /// Opens a memory pool file and returns an instance of
                /// [`Allocator`](#) if success. The pool remains open as long
                /// as the instance lives.
                #[track_caller]
                pub fn open_impl(filename: &str, no_check: bool) -> Result<PoolGuard<Self>> {
                    let metadata = std::fs::metadata(filename);
                    if let Err(e) = &metadata {
                        Err(format!("{}", e))
                    } else {
                        let metadata = metadata.unwrap();
                        assert!(metadata.is_file());
                        if metadata.len() < 8 {
                            Err("Invalid pool file".to_string())
                        } else {
                            let path = PathBuf::from(filename);
                            let file = OpenOptions::new()
                                .read(true)
                                .write(true)
                                .create(true)
                                .open(&path)
                                .unwrap();
    
                            let mut mmap =
                                unsafe { memmap::MmapOptions::new().map_mut(&file).unwrap() };
    
                            let raw_offset = mmap.get_mut(0).unwrap();
    
                            let id = std::any::type_name::<BuddyAllocInner>();
                            let mut s = DefaultHasher::new();
                            id.hash(&mut s);
                            let id = s.finish();
    
                            let inner = unsafe {
                                read::<BuddyAllocInner>(raw_offset)
                            };
                            if !no_check {
                                assert_eq!(
                                    inner.magic_number, id,
                                    "Invalid magic number for the pool image file"
                                );
                            }
    
                            let base = raw_offset as *mut _ as u64;
                            unsafe {
                                inner.gen = MAX_GEN.max(inner.gen + 1);
                                inner.tx_gen = 0;
                                MAX_GEN = inner.gen;
                                BUDDY_START = base;
                                BUDDY_VALID_START = base
                                    + mem::size_of::<BuddyAllocInner>() as u64
                                    + mem::size_of::<BuddyAlg<Self>>() as u64;
                                BUDDY_END = BUDDY_START + inner.size as u64 + 1;
                                BUDDY_INNER = Some(inner);
                                let mut vdata = match VDATA.lock() {
                                    Ok(g) => g,
                                    Err(p) => p.into_inner()
                                };
                                *vdata = Some(VData::new(mmap, filename));
                            }
    
                            Ok(PoolGuard::<Self>::new())
                        }
                    }
                }
            }
    
            unsafe impl MemPoolTraits for $name {
                #[inline]
                fn name() -> &'static str {
                    stringify!($mod)
                }
    
                /// Formats the image file
                unsafe fn format(filename: &str) -> Result<()> {
                    if Path::new(filename).exists() {
                        let file = OpenOptions::new()
                            .read(true)
                            .write(true)
                            .create(true)
                            .open(filename);
                        if let Err(e) = &file {
                            Err(format!("{}", e))
                        } else {
                            let file = file.unwrap();
                            let mut len = file.metadata().unwrap().len() as usize;
                            if len < 8 {
                                len = 10 * 1024 * 1024;
                                file.set_len(len as u64).unwrap();
                            }
    
                            let mut mmap = memmap::MmapOptions::new().map_mut(&file).unwrap();
                            let begin = mmap.get_mut(0).unwrap();
                            std::ptr::write_bytes(begin, 0xff, 8);
                            BUDDY_START = begin as *const _ as u64;
                            BUDDY_END = u64::MAX;
    
                            let inner = read::<BuddyAllocInner>(begin);
                            inner.init(len);
                            mmap.flush().unwrap();
                            Ok(())
                        }
                    } else {
                        Err("Image file does not exist".to_string())
                    }
                }
    
                #[inline]
                #[track_caller]
                fn gen() -> u32 {
                    static_inner!(BUDDY_INNER, inner, { inner.gen })
                }
    
                #[inline]
                #[track_caller]
                fn tx_gen() -> u32 {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.tx_gen += 1;
                        inner.tx_gen
                    })
                }
    
                #[track_caller]
                fn size() -> usize {
                    static_inner!(BUDDY_INNER, inner, { inner.size })
                }
    
                #[inline]
                #[track_caller]
                fn available() -> usize {
                    static_inner!(BUDDY_INNER, inner, {
                        let mut sum = 0;
                        for i in 0..inner.zone.count() {
                            sum += inner.zone[i].available();
                        }
                        sum
                    })
                }
    
                #[track_caller]
                fn used() -> usize {
                    static_inner!(BUDDY_INNER, inner, {
                        let mut sum = 0;
                        for i in 0..inner.zone.count() {
                            sum += inner.zone[i].used();
                        }
                        sum
                    })
                }
    
                #[inline]
                fn rng() -> Range<u64> {
                    unsafe { BUDDY_VALID_START..BUDDY_END }
                }
    
                #[inline]
                fn start() -> u64 {
                    unsafe { BUDDY_START }
                }
    
                #[inline]
                fn end() -> u64 {
                    unsafe { BUDDY_END }
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize, usize) {
                    let _perf = $crate::__cfg_stat_perf!($crate::stat::Measure::<Self>::Alloc(std::time::Instant::now()));
    
                    static_inner!(BUDDY_INNER, inner, {
                        let cpu = cpu();
                        let cnt = inner.zone.count();
                        for i in 0..cnt {
                            let z = (cpu+i)%cnt;
                            let a = inner.zone[z].alloc_impl(size, false);
                            if a != u64::MAX {
                                return (Self::get_mut_unchecked(a), a, size, z);
                            }
                        }
                        eprintln!(
                            "No space left (requested = {}, available= {})",
                            size, Self::available()
                        );
                        (std::ptr::null_mut(), u64::MAX, 0, 0)
                    })
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn pre_dealloc(ptr: *mut u8, size: usize) -> usize {
                    let _perf = $crate::__cfg_stat_perf!($crate::stat::Measure::<Self>::Dealloc(std::time::Instant::now()));
    
                    static_inner!(BUDDY_INNER, inner, {
                        let off = Self::off(ptr).expect("invalid pointer");
                        let (zone,zidx) = inner.zone.from_off(off);
                        $crate::__cfg_check_access_violation!({
                            if zone.is_allocated(off, size) {
                                zone.dealloc_impl(off, size, false);
                            } else {
                                panic!("offset @{} ({}) was not allocated", off, size);
                            }
                        }, {
                            zone.dealloc_impl(off, size, false);
                        });
                        zidx
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn log64(off: u64, val: u64, z: usize) {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.zone[z].log(off, val);
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn drop_on_failure(off: u64, len: usize, z: usize) {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.zone[z].drop_on_failure(off, len);
                    })
                }
    
                #[inline]
                #[track_caller]
                fn zone(off: u64) -> usize {
                    static_inner!(BUDDY_INNER, inner, {
                        off as usize / inner.zone.quota()
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn prepare(z: usize) {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.zone[z].prepare();
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn perform(z: usize) {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.zone[z].perform();
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn discard(z: usize) {
                    static_inner!(BUDDY_INNER, inner, {
                        inner.zone[z].discard();
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                fn allocated(off: u64, _len: usize) -> bool {
                    static_inner!(BUDDY_INNER, _inner, {
                        if off >= Self::end() {
                            false
                        } else if Self::contains(off + Self::start()) {
                            $crate::__cfg_check_access_violation!(
                                { _inner.zone.from_off(off).0.is_allocated(off, _len) },
                                { true })
                        } else {
                            false
                        }
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                fn verify() -> bool {
                    static_inner!(BUDDY_INNER, inner, {
                        for i in 0..inner.zone.count() {
                            if !inner.zone[i].verify() {
                                return false;
                            }
                        }
                        true
                    })
                }
    
                #[inline]
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn journals_head() -> &'static u64 {
                    static_inner!(BUDDY_INNER, inner, {
                        &inner.journals
                    })
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn drop_journal(journal: &mut Journal) {
                    let _vdata = match VDATA.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner()
                    };
                    static_inner!(BUDDY_INNER, inner, {
                        let off = Self::off(journal).unwrap();
                    
                        $crate::__cfg_pin_journals!({
                            journal.drop_pages();
                        });
    
                        let z = Self::pre_dealloc(journal as *mut _ as *mut u8, mem::size_of::<Journal>());
                        if inner.journals == off {
                            Self::log64(Self::off_unchecked(&inner.journals), journal.next_off(), z);
                        }
                        if let Ok(prev) = Self::deref_mut::<Journal>(journal.prev_off()) {
                            Self::log64(Self::off_unchecked(prev.next_off_ref()), journal.next_off(), z);
                        }
                        if let Ok(next) = Self::deref_mut::<Journal>(journal.next_off()) {
                            Self::log64(Self::off_unchecked(next.prev_off_ref()), journal.prev_off(), z);
                        }
                        Self::perform(z);
                    });
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                unsafe fn journals<T, F: Fn(&mut HashMap<ThreadId, (u64, i32)>)->T>(f: F)->T{
                    let mut vdata = match VDATA.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner()
                    };
                    if let Some(vdata) = &mut *vdata {
                        f(&mut vdata.journals)
                    } else {
                        panic!("No memory pool is open or the root object is moved to a transaction. Try cloning the root object instead of moving it to a transaction.");
                    }
                }

                unsafe fn dealloc_history() -> *mut HashSet<u64> {
                    let mut vdata = match VDATA.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner()
                    };
                    if let Some(vdata) = &mut *vdata {
                        return &mut vdata.check_double_free;
                    } else {
                        panic!("No memory pool is open or the root object is moved to a transaction. Try cloning the root object instead of moving it to a transaction.");
                    }
                }
    
                #[allow(unused_unsafe,unused_braces)]
                unsafe fn recover() {
                    static_inner!(BUDDY_INNER, inner, {
                        let info_level = std::env::var("RECOVERY_INFO")
                            .unwrap_or("0".to_string())
                            .parse::<u32>()
                            .expect("RECOVERY_INFO should be an unsigned integer");
                        
                        if info_level > 0 {
                            for i in 0..inner.zone.count() {
                                eprintln!("{:=^60}", format!(" Restore Allocator (Zone {}) ", i));
                                eprintln!("{}", inner.zone[i].recovery_info(info_level));
                            }
    
                            let mut curr = inner.journals;
                            while let Ok(j) = Self::deref_mut::<Journal>(curr) {
                                eprintln!("{:-^60}\n{}", format!(" Journal @({}) ", curr), j.recovery_info(info_level));
                                curr = j.next_off();
                            }
                        }
    
                        for i in 0..inner.zone.count() {
                            inner.zone[i].recover();
                        }
    
                        $crate::__cfg_check_allocator_cyclic_links!({
                            debug_assert!(Self::verify());
                        });
                        
                        #[allow(unused_mut,unused_variables)]
                        let mut check_double_free = __cfg_delete_history!({
                            std::collections::HashSet::<u64>::new()
                        }, { () });
                        
    
                        while let Ok(logs) = Self::deref_mut::<Journal>(inner.journals) {
    
                            $crate::__cfg_verbose!({
                                if *utils::VERBOSE {
                                    println!("{:?}", logs);
                                }
                            });
    
                            $crate::__cfg_check_allocator_cyclic_links!({
                                debug_assert!(Self::verify());
                            });
    
                            __cfg_delete_history!({
                                logs.recover(&mut check_double_free);
                            }, {
                                logs.recover();
                            });
    
                            $crate::__cfg_check_allocator_cyclic_links!({
                                debug_assert!(Self::verify());
                            });
    
                            __cfg_delete_history!({
                                logs.clear(&mut check_double_free);
                            }, {
                                logs.clear();
                            });
    
                            $crate::__cfg_check_allocator_cyclic_links!({
                                debug_assert!(Self::verify());
                            });
    
                            $crate::__cfg_pin_journals!({
                                Self::drop_journal(logs);
                            });
                        }
                    })
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                fn open<'a, U: 'a + PSafe + RootObj<Self>>(
                    path: &str,
                    flags: u32,
                ) -> Result<RootCell<'a, U, Self>> {
                    let slf = Self::open_no_root(path, flags)?;
                    static_inner!(BUDDY_INNER, inner, {
                        // Replace it with std::any::TypeId::of::<U>() when it
                        // is available in the future for non-'static types
                        let id = format!("{} ({})", std::any::type_name::<U>(),
                            mem::size_of::<U>());
                        let mut s = DefaultHasher::new();
                        id.hash(&mut s);
                        let id = s.finish();
                        if !inner.has_root() {
                            if mem::size_of::<U>() == 0 {
                                Err("root type cannot be a ZST".to_string())
                            } else {
                                let root_off = Self::transaction(move |j| {
                                    let ptr = Self::new(U::init(j), j);
                                    Self::off_unchecked(ptr)
                                })
                                .unwrap();
                                let ptr = Self::get_unchecked(root_off);
                                inner.flags |= FLAG_HAS_ROOT;
                                inner.root_obj = root_off;
                                inner.root_type_id = id;
                                persist_obj(inner, true);
                                Ok(RootCell::new(ptr, Arc::new(slf)))
                            }
                        } else {
                            if inner.root_type_id == id {
                                Ok(RootCell::new(
                                    Self::deref::<U>(inner.root_obj)?,
                                    Arc::new(slf),
                                ))
                            } else {
                                Err("Incompatible root type".to_string())
                            }
                        }
                    })
                }
    
                #[inline]
                fn is_open() -> bool {
                    unsafe { BUDDY_INNER.is_some() }
                }
    
                #[allow(unused_unsafe)]
                #[track_caller]
                fn open_no_root(path: &str, flags: u32) -> Result<PoolGuard<Self>> {
                    unsafe {
                        while OPEN.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_err() {}
                        if !Self::running_transaction() {
                            if flags == open_flags::O_READINFO {
                                Self::open_impl(path, true)
                            } else if let Ok(_) = Self::apply_flags(path, flags) {
                                let res = Self::open_impl(path, false);
                                if res.is_ok() {
                                    Self::recover();
                                }
                                res
                            } else {
                                OPEN.store(false, Ordering::Release);
                                Err("Could not open file".to_string())
                            }
                        } else {
                            OPEN.store(false, Ordering::Release);
                            Err("An uncommitted transaction exists in the pool"
                                .to_string())
                        }
                    }
                }
    
                #[allow(unused_unsafe)]
                unsafe fn close() -> Result<()> {
                    if OPEN.load(Ordering::Acquire) {
                        let mut vdata = match VDATA.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner()
                        };
                        *vdata = None;
                        BUDDY_INNER = None;
                        OPEN.store(false, Ordering::Release);
                        Ok(())
                    } else {
                        Err("Pool was already closed".to_string())
                    }
                }
    
                fn stat_footprint() -> usize {
                    $crate::__cfg_stat_footprint!({
                        static_inner!(BUDDY_INNER, inner, { inner.zone.stat_footprint() })
                    }, {
                        unimplemented!()
                    })
                }
    
                fn print_info() {
                    println!("{:=^80}", " All Zones ");
                    println!("      Total: {} bytes", Self::size());
                    println!("       Used: {} bytes", Self::used());
                    println!("  Available: {} bytes", Self::available());
    
                    static_inner!(BUDDY_INNER, inner, { 
                        for i in 0..inner.zone.count() {
                            println!("{:=^80}", format!(" Persistent Memory Zone #{} ", i));
                            println!("       Total      {}", inner.zone[i].size());
                            println!("        Used      {}", inner.zone[i].used());
                            println!("   Available      {}", inner.zone[i].available());
                            inner.zone[i].print();
                        }
                    })
                }
            }
    
            /// Compact form of [`Pbox`](../../boxed/struct.Pbox.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type Pbox<T> = $crate::Pbox<T, $name>;
    
            /// Compact form of [`Prc`](../../prc/struct.Prc.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type Prc<T> = $crate::prc::Prc<T, $name>;
    
            /// Compact form of [`Parc`](../../sync/struct.Parc.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type Parc<T> = $crate::sync::Parc<T, $name>;
    
            /// Compact form of [`PMutex`](../../sync/struct.PMutex.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PMutex<T> = $crate::sync::PMutex<T, $name>;
    
            /// Compact form of [`PCell`](../../cell/struct.PCell.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PCell<T> = $crate::PCell<T, $name>;
    
            /// Compact form of [`LogNonNull`](../../ptr/struct.LogNonNull.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PNonNull<T> = $crate::ptr::LogNonNull<T, $name>;
    
            /// Compact form of [`PRefCell`](../../cell/struct.PRefCell.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PRefCell<T> = $crate::PRefCell<T, $name>;
    
            /// Compact form of [`Ref`](../../cell/struct.Ref.html)
            /// `<'b, T, `[`Allocator`](./struct.Allocator.html)`>`.
            pub type PRef<'b, T> = $crate::Ref<'b, T, $name>;
    
            /// Compact form of [`RefMut`](../../cell/struct.Mut.html)
            /// `<'b, T, `[`Allocator`](./struct.Allocator.html)`>`.
            pub type PRefMut<'b, T> = $crate::RefMut<'b, T, $name>;
    
            /// Compact form of [`VCell`](../../cell/struct.VCell.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type VCell<T> = $crate::VCell<T, $name>;
    
            /// Compact form of [`TCell`](../../cell/struct.TCell.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type TCell<T> = $crate::TCell<T, $name>;
    
            /// Compact form of [`Vec`](../../vec/struct.Vec.html)
            /// `<T,`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PVec<T> = $crate::vec::Vec<T, $name>;
    
            /// Compact form of [`String`](../../str/struct.String.html)
            /// `<`[`Allocator`](./struct.Allocator.html)`>`.
            pub type PString = $crate::PString<$name>;
    
            /// Compact form of [`Journal`](../../stm/struct.Journal.html)
            /// `<`[`Allocator`](./struct.Allocator.html)`>`.
            pub type Journal = $crate::stm::Journal<$name>;
    
            pub mod prc {
                /// Compact form of [`prc::Weak`](../../../prc/struct.Weak.html)
                /// `<`[`Allocator`](./struct.Allocator.html)`>`.
                pub type PWeak<T> = $crate::prc::Weak<T, super::$name>;
    
                /// Compact form of [`prc::VWeak`](../../../prc/struct.VWeak.html)
                /// `<`[`Allocator`](../struct.Allocator.html)`>`.
                pub type VWeak<T> = $crate::prc::VWeak<T, super::$name>;
            }
    
            pub mod parc {
                /// Compact form of [`sync::Weak`](../../../sync/struct.Weak.html)
                /// `<`[`Allocator`](../struct.Allocator.html)`>`.
                pub type PWeak<T> = $crate::sync::Weak<T, super::$name>;
    
                /// Compact form of [`sync::VWeak`](../../../sync/struct.VWeak.html)
                /// `<`[`Allocator`](../struct.Allocator.html)`>`.
                pub type VWeak<T> = $crate::sync::VWeak<T, super::$name>;
            }
        }
    };
    ($mod:ident) => {
        $crate::pool!($mod, Allocator);
    };
}

#[cfg(feature = "verbose")]
pub fn debug_alloc<A: MemPool>(addr: u64, len: usize, pre: usize, post: usize) {
    crate::log!(A, Green, "", "PRE: {:<6}  ({:>6x}:{:<6x}) = {:<6} POST = {:<6}",
        pre, addr, addr + len as u64 - 1, len, post);
}

#[cfg(feature = "verbose")]
pub fn debug_dealloc<A: MemPool>(addr: u64, len: usize, pre: usize, post: usize) {
    crate::log!(A, Red, "DEALLOC", "PRE: {:<6}  ({:>6x}:{:<6x}) = {:<6} POST = {:<6}",
        pre, addr, addr + len as u64 - 1, len, post);
}
