use std::marker::PhantomData;
use std::panic::RefUnwindSafe;
use crate::cell::{RootCell, RootObj};
use crate::result::Result;
use crate::stm::*;
use crate::utils::*;
use crate::*;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::ops::Range;
use std::panic::UnwindSafe;
use std::path::Path;
use std::thread::ThreadId;
use std::{mem, ptr};

/// Default pool memory size to be used while creating a new pool
pub const DEFAULT_POOL_SIZE: u64 = 8 * 1024 * 1024;

/// Open pool flags
pub mod open_flags {
    /// Open Flag: Create the pool memory file
    pub const O_C: u32 = 0x00000001;

    /// Open Flag: Formats the pool memory file if file exists, otherwise error
    pub const O_F: u32 = 0x00000002;

    /// Open Flag: Creates pool memory file only if it does not exist
    pub const O_CNE: u32 = 0x00000004;

    /// Open Flag: Creates and formats a new file
    pub const O_CF: u32 = O_C | O_F;

    /// Open Flag: Creates and formats pool memory file only if it does not exist
    pub const O_CFNE: u32 = O_CNE | O_F;

    /// Open Flag: Creates a pool memory file of size 1GB
    pub const O_1GB: u32 = 0x00000010;

    /// Open Flag: Creates a pool memory file of size 2GB
    pub const O_2GB: u32 = 0x00000020;

    /// Open Flag: Creates a pool memory file of size 4GB
    pub const O_4GB: u32 = 0x00000040;

    /// Open Flag: Creates a pool memory file of size 8GB
    pub const O_8GB: u32 = 0x00000080;

    /// Open Flag: Creates a pool memory file of size 16GB
    pub const O_16GB: u32 = 0x00000100;

    /// Open Flag: Creates a pool memory file of size 32GB
    pub const O_32GB: u32 = 0x00000200;

    /// Open Flag: Creates a pool memory file of size 64GB
    pub const O_64GB: u32 = 0x00000400;

    /// Open Flag: Creates a pool memory file of size 128GB
    pub const O_128GB: u32 = 0x00000800;

    /// Open Flag: Creates a pool memory file of size 256GB
    pub const O_256GB: u32 = 0x00001000;

    /// Open Flag: Creates a pool memory file of size 512GB
    pub const O_512GB: u32 = 0x00002000;

    /// Open Flag: Creates a pool memory file of size 1TB
    pub const O_1TB: u32 = 0x00004000;

    /// Open Flag: Creates a pool memory file of size 2TB
    pub const O_2TB: u32 = 0x00008000;

    /// Open Flag: Creates a pool memory file of size 4TB
    pub const O_4TB: u32 = 0x00010000;

    /// Open Flag: Creates a pool memory file of size 8TB
    pub const O_8TB: u32 = 0x00020000;

    /// Open Flag: Creates a pool memory file of size 16TB
    pub const O_16TB: u32 = 0x00040000;

    /// Open Flag: Creates a pool memory file of size 32TB
    pub const O_32TB: u32 = 0x00080000;

    /// Open Flag: Creates a pool memory file of size 64TB
    pub const O_64TB: u32 = 0x00100000;

    /// Open Flag: Open only to read info
    pub const O_READINFO: u32 = u32::MAX;
}

use open_flags::*;

/// Shows that the pool has a root object
pub const FLAG_HAS_ROOT: u64 = 0x0000_0001;

/// This macro can be used to access static data of an arbitrary allocator
#[macro_export]
macro_rules! static_inner {
    ($id:ident, $inner:ident, $body:block) => {
        unsafe {
            if let Some($inner) = $id {
                let $inner = &mut *$inner;
                $body
            } else {
                panic!("No memory pool is open or the root object is moved to a transaction. Try cloning the root object instead of moving it to a transaction.");
            }
        }
    };
}

/// Persistent Memory Pool
///
/// This trait can be used to define a persistent memory pool type. The
/// methods of `MemPool` trait do not have a reference to self in order to make
/// sure that all information that it works with, including the virtual address
/// boundaries, are static. Therefore, all objects with the same memory
/// allocator will share a unique memory pool type. Having a strong set of type
/// checking rules, Rust prevents referencing from one memory pool to another.
///
/// To implement a new memory pool, you should define a new type with static
/// values, that implements `MemPool`. You may redefine the default allocator as
/// a new pool using [`pool!()`] which creates a pool module and generates the
/// necessary code segments of type [`Allocator`].
///
/// # Examples
/// The following example shows how to use `MemPool` to track allocations of a
/// single numerical object of type `i32`.
///
/// ```
/// # use corundum::alloc::MemPool;
/// # use corundum::stm::Journal;
/// # use corundum::result::Result;
/// # use std::ops::Range;
/// use std::alloc::{alloc,dealloc,realloc,Layout};
///
/// struct TrackAlloc {}
///
/// unsafe impl MemPool for TrackAlloc {
///     fn rng() -> Range<u64> { 0..u64::MAX }
///     unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize, usize) {
///         let p = alloc(Layout::from_size_align_unchecked(size, 4));
///         println!("A block of {} bytes is allocated at {}", size, p as u64);
///         (p, p as u64, size, 0)
///     }
///     unsafe fn pre_dealloc(p: *mut u8, size: usize) -> usize {
///         println!("A block of {} bytes at {} is deallocated", size, p as u64);
///         dealloc(p, Layout::from_size_align_unchecked(size, 1));
///         0
///     }
/// }
///
/// unsafe {
///     let (p, _, _) = TrackAlloc::alloc(1);
///     *p = 10;
///     println!("loc {} contains {}", p as u64, *p);
///     TrackAlloc::dealloc(p, 1);
/// }
/// ```
/// 
/// The following example shows how to use [`pool!()`] to define a multiple
/// pools.
/// 
/// ```
/// # use corundum::alloc::*;
/// # use corundum::*;
/// // Declare p1 module
/// pool!(p1);
/// 
/// // Declare p2 module
/// pool!(p2);
/// 
/// let _pool1 = p1::Allocator::open_no_root("p1.pool", O_CF).unwrap();
/// let _pool2 = p2::Allocator::open_no_root("p2.pool", O_CF).unwrap();
/// 
/// transaction(|j| {
///     // Create a Pbox object in p1
///     let b = p1::Pbox::new(10, j);
/// }).unwrap();
/// 
/// transaction(|j| {
///     // Create a Prc object in p2
///     let p = p2::Prc::new(10, j);
/// }).unwrap();
/// ```
///
/// # Safety
///
/// This is the developer's responsibility to manually drop allocated objects.
/// One way for memory management is to use pointer wrappers that implement
/// [`Drop`] trait and deallocate the object on drop. Unsafe
/// methods does not guarantee persistent memory safety.
///
/// `pmem` crate provides `Pbox`, `Prc`, and `Parc` for memory management using
/// RAII. They internally use the unsafe methods.
/// 
/// [`pool!()`]: ./default/macro.pool.html
/// [`Allocator`]: ../default/struct.Allocator.html
pub unsafe trait MemPoolTraits
where
    Self: 'static + Sized,
{
    /// Returns the name of the pool type
    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Opens a new pool without any root object. This function is for testing 
    /// and is not useful in real applications as none of the allocated
    /// objects in persistent region is durable. The reason is that they are not
    /// reachable from a root object as it doesn't exists. All objects can live
    /// only in the scope of a transaction.
    /// 
    /// # Flags
    ///   * O_C:    create a memory pool file if not exists
    ///   * O_F:    format the memory pool file
    ///   * O_CNE:  create a memory pool file if not exists
    ///   * O_CF:   create and format a new memory pool file
    ///   * O_CFNE: create and format a memory pool file only if not exists
    /// 
    /// See [`open_flags`](./open_flags/index.html) for more options.
    fn open_no_root(_path: &str, _flags: u32) -> Result<PoolGuard<Self>> {
        unimplemented!()
    }

    /// Commits all changes and clears the logs for all threads
    ///
    /// This method should be called while dropping the `MemPool` object to
    /// make sure that all uncommitted changes outside transactions, such as
    /// reference counters, are persistent.
    unsafe fn close() -> Result<()> {
        unimplemented!()
    }

    /// Returns the zone index corresponding to a given address
    #[inline]
    fn zone(_off: u64) -> usize {
        0
    }

    /// Opens a pool and retrieves the root object
    ///
    /// The root type should implement [`RootObj`] trait in order to create a
    /// root object on its absence. This function [creates and] returns an
    /// immutable reference to the root object. The pool remains open as long as
    /// the root object is in the scope. Like other persistent objects, the root
    /// object is immutable and it is modifiable via interior mutability.
    /// 
    /// # Flags
    ///   * O_C:    create a memory pool file if not exists
    ///   * O_F:    format the memory pool file
    ///   * O_CNE:  create a memory pool file if not exists
    ///   * O_CF:   create and format a new memory pool file
    ///   * O_CFNE: create and format a memory pool file only if not exists
    /// 
    /// See [`open_flags`](./open_flags/index.html) for more options.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    ///
    /// let root = Allocator::open::<i32>("foo.pool", O_CF).unwrap();
    ///
    /// assert_eq!(*root, i32::default());
    /// ```
    ///
    /// ## Single-thread Shared Root Object
    ///
    /// [`Prc`]`<`[`PCell`]`<T>>` can be used in order to have a mutable shared
    /// root object, as follows.
    ///
    /// ```
    /// use corundum::default::*;
    ///
    /// type Root = Prc<PCell<i32>>;
    ///
    /// let root = Allocator::open::<Root>("foo.pool", O_CF).unwrap();
    ///
    /// let data = root.get();
    ///
    /// if data == i32::default() {
    ///     println!("Initializing data");
    ///     // This block runs only once to initialize the root object
    ///     transaction(|j| {
    ///         root.set(10, j);
    ///     }).unwrap();
    /// }
    ///
    /// assert_eq!(root.get(), 10);
    /// ```
    ///
    /// ## Thread-safe Root Object
    ///
    /// If you need a thread-safe root object, you may want to wrap the root object
    /// in [`Parc`]`<`[`PMutex`]`<T>>`, as shown in the example below:
    ///
    /// ```
    /// use corundum::default::*;
    /// use std::thread;
    ///
    /// type Root = Parc<PMutex<i32>>;
    ///
    /// let root = Allocator::open::<Root>("foo.pool", O_CF).unwrap();
    ///
    /// let mut threads = vec!();
    ///
    /// for _ in 0..10 {
    ///     let root = Parc::demote(&root);
    ///     threads.push(thread::spawn(move || {
    ///         transaction(|j| {
    ///             if let Some(root) = root.promote(j) {
    ///                 let mut root = root.lock(j);
    ///                 *root += 10;
    ///             }
    ///         }).unwrap();
    ///     }));
    /// }
    ///
    /// for thread in threads {
    ///     thread.join().unwrap();
    /// }
    ///
    /// transaction(|j| {
    ///     let data = root.lock(j);
    ///     assert_eq!(*data % 100, 0);
    /// }).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// * A volatile memory pool (e.g. `Heap`) doesn't have a root object.
    /// * The pool should be open before accessing the root object.
    ///
    /// [`RootObj`]: ../stm/trait.RootObj.html
    /// [`Prc`]: ../prc/struct.Prc.html
    /// [`Parc`]: ../sync/parc/struct.Parc.html
    /// [`PCell`]: ./default/type.PCell.html
    /// [`PRefCell`]: ./default/type.PRefCell.html
    /// [`PMutex`]: ./default/type.PMutex.html
    fn open<'a, U: 'a + PSafe + RootObj<Self>> (
        _path: &str,
        _flags: u32,
    ) -> Result<RootCell<'a, U, Self>> where Self: MemPool {
        unimplemented!()
    }

    /// Returns true if the pool is open
    fn is_open() -> bool {
        unimplemented!()
    }

    /// Formats the memory pool file
    unsafe fn format(_path: &str) -> Result<()> {
        unimplemented!()
    }

    /// Applies open pool flags
    unsafe fn apply_flags(path: &str, flags: u32) -> Result<()> {
        let mut size: u64 = flags as u64 >> 4;
        if size.count_ones() > 1 {
            return Err("Cannot have multiple size flags".to_string());
        } else if size == 0 {
            size = DEFAULT_POOL_SIZE;
        } else {
            if flags & (O_C | O_CNE) == 0 {
                return Err("Cannot use size flag without a create flag".to_string());
            }
            size <<= 30;
        }
        let mut format = !Path::new(path).exists() && ((flags & O_F) != 0);
        if ((flags & O_C) != 0) || ((flags & O_CNE != 0) && !Path::new(path).exists()) {
            let _=std::fs::remove_file(path);
            create_file(path, size)?;
            format = (flags & O_F) != 0;
        }
        if format {
            Self::format(path)?;
        }
        Ok(())
    }

    /// Indicates if the given offset is allocated
    #[inline]
    fn allocated(_off: u64, _len: usize) -> bool {
        true
    }

    /// Indicates if there the pool is in a good shape
    #[inline]
    fn verify() -> bool {
        true
    }

    /// Translates raw pointers to memory offsets
    ///
    /// # Safety
    ///
    /// The raw pointer should be in the valid range
    #[inline]
    unsafe fn off_unchecked<T: ?Sized>(x: *const T) -> u64 {
        (x as *const u8 as u64) - Self::start()
    }

    /// Acquires a reference pointer to the object
    ///
    /// # Safety
    ///
    /// The offset should be in the valid address range
    #[inline]
    unsafe fn get_unchecked<'a, T: 'a + ?Sized>(off: u64) -> &'a T {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Deref(std::time::Instant::now());

        #[cfg(any(feature = "check_access_violation", debug_assertions))]
        assert!( Self::allocated(off, 1), "Access Violation (0x{:x})", off );

        utils::read_addr(Self::start() + off)
    }

    /// Acquires a mutable reference to the object
    ///
    /// # Safety
    ///
    /// The offset should be in the valid address range
    #[inline]
    #[track_caller]
    unsafe fn get_mut_unchecked<'a, T: 'a + ?Sized>(off: u64) -> &'a mut T {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Deref(std::time::Instant::now());

        #[cfg(any(feature = "check_access_violation", debug_assertions))]
        assert!( Self::allocated(off, 1), "Access Violation (0x{:x})", off );

        utils::read_addr(Self::start() + off)
    }

    /// Acquires a reference to the slice
    ///
    /// # Safety
    ///
    /// The offset should be in the valid address range
    #[inline]
    unsafe fn deref_slice_unchecked<'a, T: 'a>(off: u64, len: usize) -> &'a [T] {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Deref(std::time::Instant::now());

        if off == u64::MAX {
            &[]
        } else {
            let ptr = utils::read_addr(Self::start() + off);
            let res = std::slice::from_raw_parts(ptr, len);

            #[cfg(any(feature = "check_access_violation", debug_assertions))]
            assert!(
                Self::allocated(off, mem::size_of::<T>().max(1) * len),
                "Access Violation (0x{:x}..0x{:x})",
                off,
                off.checked_add((mem::size_of::<T>().max(1) * len) as u64 - 1).unwrap_or_default()
            );

            res
        }
    }

    /// Acquires a mutable reference to the slice
    ///
    /// # Safety
    ///
    /// The offset should be in the valid address range
    #[inline]
    unsafe fn deref_slice_unchecked_mut<'a, T: 'a>(off: u64, len: usize) -> &'a mut [T] {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Deref(std::time::Instant::now());

        if off == u64::MAX {
            &mut []
        } else {
            let ptr = utils::read_addr(Self::start() + off);
            let res = std::slice::from_raw_parts_mut(ptr, len);

            #[cfg(any(feature = "check_access_violation", debug_assertions))]
            assert!(
                Self::allocated(off, mem::size_of::<T>().max(1) * len),
                "Access Violation (0x{:x}..0x{:x})",
                off,
                off + (mem::size_of::<T>().max(1) * len) as u64 - 1
            );

            res
        }
    }

    /// Acquires a reference to the object
    #[inline]
    unsafe fn deref<'a, T: 'a>(off: u64) -> Result<&'a T> {
        if Self::allocated(off, mem::size_of::<T>()) {
            Ok(Self::get_unchecked(off))
        } else {
            Err(format!("Access Violation (0x{:x})", off))
        }
    }

    /// Acquires a mutable reference pointer to the object
    #[inline]
    unsafe fn deref_mut<'a, T: 'a>(off: u64) -> Result<&'a mut T> {
        if Self::allocated(off, mem::size_of::<T>()) {
            Ok(Self::get_mut_unchecked(off))
        } else {
            Err(format!("Access Violation (0x{:x})", off))
        }
    }

    /// Translates raw pointers to memory offsets
    #[inline]
    fn off<T: ?Sized>(x: *const T) -> Result<u64> {
        if Self::valid(x) {
            Ok(x as *const u8 as u64 - Self::start())
        } else {
            Err(format!("out of valid range ({:p})", x).to_string())
        }
    }

    /// Valid Virtual Address Range
    fn rng() -> Range<u64> {
        Self::start()..Self::end()
    }

    /// Start of virtual address range
    #[inline]
    fn start() -> u64 {
        Self::rng().start
    }

    /// End of virtual address range
    #[inline]
    fn end() -> u64 {
        Self::rng().end
    }

    /// Total size of the memory pool
    fn size() -> usize {
        unimplemented!()
    }

    /// Available space in the pool
    fn available() -> usize {
        unimplemented!()
    }

    /// Total occupied space
    fn used() -> usize {
        Self::size() - Self::available()
    }

    /// Checks if the reference `p` belongs to this pool
    #[inline]
    fn valid<T: ?Sized>(p: *const T) -> bool {
        let rng = Self::rng();
        let start = p as *const u8 as u64;
        // let end = start + std::mem::size_of_val(p) as u64;
        start >= rng.start && start < rng.end
        // && end >= rng.start && end < rng.end
    }

    /// Checks if `addr` is in the valid address range if this allocator
    ///
    /// `addr` contains the scalar of a virtual address. If you have a raw
    /// fat pointer of type T, you can obtain its virtual address by converting
    /// it into a thin pointer and then `u64`.
    ///
    /// # Examples
    ///
    /// ```
    /// let p = Box::new(1);
    /// println!("Address {:#x} contains value '{}'", p.as_ref() as *const _ as u64, *p);
    /// ```
    #[inline]
    fn contains(addr: u64) -> bool {
        let rng = Self::rng();
        addr >= rng.start && addr < rng.end
    }

    /// Allocate memory as described by the given `size`.
    ///
    /// Returns a pointer to newly-allocated memory.
    ///
    /// # Safety
    ///
    /// This function is unsafe because undefined behavior can result
    /// if the caller does not ensure that `size` has non-zero.
    /// The allocated block of memory may or may not be initialized.
    /// Using `alloc` may lead to memory leak if the transaction fails
    /// after this function successfully returns. To allocate memory in
    /// a failure-atomic manner, use [`pre_alloc`], [`Log::drop_on_failure`],
    /// and [`perform`] functions respectively.
    /// 
    /// [`pre_alloc`]: #method.pre_alloc
    /// [`Log::drop_on_failure`]: ../stm/struct.Log.html#method.drop_on_failure
    /// [`perform`]: #method.pre_alloc
    #[inline]
    #[track_caller]
    unsafe fn alloc(size: usize) -> (*mut u8, u64, usize) {
        let (p, off, len, z) = Self::pre_alloc(size);
        Self::drop_on_failure(off, len, z);
        Self::perform(z);
        (p, off, len)
    }

    /// Deallocate the block of memory at the given `ptr` pointer with the
    /// given `size`.
    ///
    /// # Safety
    ///
    /// This function is unsafe because undefined behavior can result if the
    /// caller does not ensure all of the following:
    ///
    /// * `ptr` must denote a block of memory currently allocated via this
    ///   allocator,
    ///
    /// * `size` must be the same size that was used to allocate that block
    ///   of memory.
    #[inline]
    #[track_caller]
    unsafe fn dealloc(ptr: *mut u8, size: usize) {
        Self::perform(Self::pre_dealloc(ptr, size));
    }

    /// Prepares allocation without performing it
    /// 
    /// This function is used internally for low-level atomicity in memory
    /// allocation. As an example, please see [`drop_on_failure`].
    /// 
    /// It returns a 4-tuple:
    ///     1. Raw pointer
    ///     2. Offset
    ///     3. Size
    ///     4. Zone index
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # type P = Allocator;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// unsafe {
    ///     let (ptr, _, _, z) = P::pre_alloc(8);
    ///     *ptr = 10;
    ///     P::perform(z);
    /// }
    /// ```
    /// 
    /// [`drop_on_failure`]: #method.drop_on_failure
    /// 
    unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize, usize);

    /// Prepares deallocation without performing it
    /// 
    /// This function is used internally for low-level atomicity in memory
    /// allocation. As an example, please see [`drop_on_failure`].
    /// 
    /// It returns the zone in which the deallocation happens.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # type P = Allocator;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// unsafe {
    ///     let (ptr, _, _) = P::alloc(8);
    ///     *ptr = 10;
    ///     let zone = P::pre_dealloc(ptr, 8);
    ///     assert_eq!(*ptr, 10);
    ///     P::perform(zone);
    ///     assert_ne!(*ptr, 10);
    /// }
    /// ```
    /// 
    /// [`drop_on_failure`]: #method.drop_on_failure
    /// 
    unsafe fn pre_dealloc(ptr: *mut u8, size: usize) -> usize;

    /// Adds a low-level log to update as 64-bit `obj` to `val` when 
    /// [`perform()`] is called. As an example, please see [`Log::set()`].
    /// 
    /// [`perform()`]: #method.perform
    /// [`Log::set()`]: ../stm/struct.Log.html#method.set
    /// 
    unsafe fn log64(_off: u64, _val: u64, _zone: usize) {
        unimplemented!()
    }

    /// Adds a low-level `DropOnFailure` log to perform inside the allocator. 
    /// This is internally used to atomically allocate a new objects. Calling
    /// [`perform()`] drops these logs.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # type P = Allocator;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// unsafe {
    ///     // Prepare an allocation. The allocation is not durable yet. In case
    ///     // of a crash, the prepared allocated space is gone. It is fine
    ///     // because it has not been used. The `pre_` and `perform` functions
    ///     // form a low-level atomic section.
    ///     let (obj, off, len, zone) = P::pre_alloc(1);
    /// 
    ///     // Create a low-level DropOnFailure log. This log is going to be used
    ///     // when a crash happens while performing the changes made by the
    ///     // preparation functions. If a crash happens before that, these logs
    ///     // will be discarded.
    ///     P::drop_on_failure(off, len, zone);
    ///     
    ///     // It is fine to work with the prepared raw pointer. All changes in
    ///     // the low-level atomic section are considered as part of the
    ///     // allocation and will be gone in case of a crash, as the allocation
    ///     // will be dropped.
    ///     *obj = 20;
    /// 
    ///     // Transaction ends here. The perform function sets the `operating`
    ///     // flag to show that the prepared changes are being materialized.
    ///     // This flag remains set until the end of materialization. In case
    ///     // of a crash while operating, the recovery procedure first continues
    ///     // the materialization, and then uses the `DropOnFailure` logs to
    ///     // reclaim the allocation. `perform` function realizes the changes
    ///     // made by the `pre_` function on the given memory zone.
    ///     P::perform(zone);
    /// }
    /// ```
    /// 
    /// [`perform()`]: #method.perform
    /// [`Journal`]: ../stm/journal/struct.Journal.html
    /// 
    unsafe fn drop_on_failure(_off: u64, _len: usize, _zone: usize) {}


    /// In case of not using [`pre_alloc`] or [`pre_dealloc`], starts a low-level
    /// atomic section on a given zone.
    /// 
    /// [`pre_alloc`]: #method.pre_alloc
    /// [`pre_dealloc`]: #method.pre_dealloc
    /// 
    unsafe fn prepare(_zone: usize) { }

    /// Performs the prepared operations
    /// 
    /// It materializes the changes made by [`pre_alloc`](#method.pre_alloc),
    /// [`pre_dealloc`](#method.pre_dealloc), and
    /// [`pre_realloc`](#method.pre_realloc). See [`drop_on_failure`] for more
    /// details.
    /// 
    /// [`drop_on_failure`]: #method.drop_on_failure
    /// 
    unsafe fn perform(_zone: usize) { }

    /// Discards the prepared operations
    /// 
    /// Discards the changes made by [`pre_alloc`](#method.pre_alloc),
    /// [`pre_dealloc`](#method.pre_dealloc), and
    /// [`pre_realloc`](#method.pre_realloc).  See [`drop_on_failure`] for more
    /// details.
    /// 
    /// [`drop_on_failure`]: #method.drop_on_failure
    /// 
    unsafe fn discard(_zone: usize) { }

    /// Behaves like `alloc`, but also ensures that the contents
    /// are set to zero before being returned.
    ///
    /// # Safety
    ///
    /// This function is unsafe for the same reasons that `alloc` is.
    /// However the allocated block of memory is guaranteed to be initialized.
    ///
    /// # Errors
    ///
    /// Returning a null pointer indicates that either memory is exhausted
    /// or `size` does not meet allocator's size constraints, just as in `alloc`.
    ///
    /// Clients wishing to abort computation in response to an
    /// allocation error are encouraged to call the [`handle_alloc_error`] function,
    /// rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: ../../alloc/alloc/fn.handle_alloc_error.html
    unsafe fn alloc_zeroed(size: usize) -> *mut u8 {
        let (ptr, _, _) = Self::alloc(size);
        if !ptr.is_null() {
            std::ptr::write_bytes(ptr, 0, size);
        }
        ptr
    }

    /// Allocates new memory and then places `x` into it with `DropOnFailure` log
    unsafe fn new<'a, T: PSafe + 'a>(x: T, j: &Journal<Self>) -> &'a mut T where Self: MemPool {
        debug_assert!(mem::size_of::<T>() != 0, "Cannot allocated ZST");

        let mut log = Log::drop_on_failure(u64::MAX, 1, j);
        let (p, off, len, z) = Self::atomic_new(x);
        log.set(off, len, z);
        Self::perform(z);
        p
    }

    /// Allocates a new slice and then places `x` into it with `DropOnAbort` log
    unsafe fn new_slice<'a, T: PSafe + 'a>(x: &'a [T], journal: &Journal<Self>) -> &'a mut [T] where Self: MemPool {
        debug_assert!(mem::size_of::<T>() != 0, "Cannot allocate ZST");
        debug_assert!(!x.is_empty(), "Cannot allocate empty slice");

        let mut log = Log::drop_on_abort(u64::MAX, 1, journal);
        let (p, off, size, z) = Self::atomic_new_slice(x);
        log.set(off, size, z);
        Self::perform(z);
        p
    }

    /// Allocates new memory and then copies `x` into it with `DropOnFailure` log
    unsafe fn new_copy<'a, T: 'a>(x: &T, j: &Journal<Self>) -> &'a mut T 
    where T: ?Sized, Self: MemPool {
        let s = mem::size_of_val(x);
        debug_assert!(s != 0, "Cannot allocated ZST");

        let mut log = Log::drop_on_failure(u64::MAX, 1, j);
        let (p, off, len, z) = Self::pre_alloc(s);
        if p.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, len, z);
        std::ptr::copy_nonoverlapping(x as *const T as *const u8, p, s);
        log.set(off, len, z);
        Self::perform(z);
        &mut *utils::read(p)
    }

    /// Allocates new memory and then copies `x` into it with `DropOnFailure` log
    unsafe fn new_copy_slice<'a, T: 'a>(x: &[T], j: &Journal<Self>) -> &'a mut [T] where Self: MemPool {
        let s = mem::size_of_val(x);
        debug_assert!(s != 0, "Cannot allocated ZST");

        let mut log = Log::drop_on_failure(u64::MAX, 1, j);
        let (p, off, len, z) = Self::pre_alloc(s);
        if p.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, len, z);
        std::ptr::copy_nonoverlapping(x as *const [T] as *const u8, p, s);
        log.set(off, len, z);
        Self::perform(z);
        &mut *utils::read(p)
    }

    /// Allocates new memory and then places `x` into it without realizing the allocation
    unsafe fn atomic_new<'a, T: 'a>(x: T) -> (&'a mut T, u64, usize, usize) {
        log!(Self, White, "ALLOC", "TYPE: {}", std::any::type_name::<T>());

        let size = mem::size_of::<T>();
        let (raw, off, len, z) = Self::pre_alloc(size);
        if raw.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, len, z);
        let p = &mut *utils::read(raw);
        mem::forget(ptr::replace(p, x));
        (p, off, size, z)
    }

    /// Allocates new memory and then places `x` into it without realizing the allocation
    unsafe fn atomic_new_slice<'a, T: 'a + PSafe>(x: &'a [T]) -> (&'a mut [T], u64, usize, usize) {
        log!(Self, White, "ALLOC", "TYPE: [{}; {}]", std::any::type_name::<T>(), x.len());

        let (ptr, off, size, z) = Self::pre_alloc(mem::size_of_val(x));
        if ptr.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, size, z);
        ptr::copy_nonoverlapping(
            x as *const _ as *const u8,
            ptr,
            x.len() * mem::size_of::<T>().max(1),
        );
        (
            std::slice::from_raw_parts_mut(ptr.cast(), x.len()),
            off,
            size,
            z
        )
    }

    /// Allocates new memory without copying data
    unsafe fn new_uninit<'a, T: PSafe + 'a>(j: &Journal<Self>) -> &'a mut T where Self: MemPool {
        let mut log = Log::drop_on_failure(u64::MAX, 1, j);
        let (p, off, size, z) = Self::atomic_new_uninit();
        log.set(off, size, z);
        Self::perform(z);
        p
    }

    /// Allocates new memory without copying data
    unsafe fn new_uninit_for_layout(size: usize, journal: &Journal<Self>) -> *mut u8 where Self: MemPool {
        log!(Self, White, "ALLOC", "{:?}", size);

        let mut log = Log::drop_on_abort(u64::MAX, 1, journal);
        let (p, off, len, z) = Self::pre_alloc(size);
        if p.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, len, z);
        log.set(off, len, z);
        Self::perform(z);
        p
    }

    /// Allocates new memory without copying data and realizing the allocation
    unsafe fn atomic_new_uninit<'a, T: 'a>() -> (&'a mut T, u64, usize, usize) {
        let (ptr, off, len, z) = Self::pre_alloc(mem::size_of::<T>());
        if ptr.is_null() {
            panic!("Memory exhausted");
        }
        Self::drop_on_failure(off, len, z);
        (&mut *utils::read(ptr), off, len, z)
    }

    /// Allocates new memory for value `x`
    unsafe fn alloc_for_value<'a, T: ?Sized>(x: &T) -> &'a mut T {
        let raw = Self::alloc(mem::size_of_val(x));
        if raw.0.is_null() {
            panic!("Memory exhausted");
        }
        &mut *utils::read(raw.0)
    }

    /// Creates a `DropOnCommit` log for the value `x`
    unsafe fn free<'a, T: PSafe + ?Sized>(x: &mut T) where Self: MemPool {
        // std::ptr::drop_in_place(x);
        let off = Self::off_unchecked(x);
        let len = mem::size_of_val(x);
        if std::thread::panicking() {
            Log::drop_on_abort(off, len, &*Journal::<Self>::current(true).unwrap().0);
        } else {
            Log::drop_on_commit(off, len, &*Journal::<Self>::current(true).unwrap().0);
        }
    }

    /// Creates a `DropOnCommit` log for the value `x`
    unsafe fn free_slice<'a, T: PSafe>(x: &[T]) where Self: MemPool {
        // eprintln!("FREEING {} of size {}", x as *mut u8 as u64, len);
        if x.len() > 0 {
            let off = Self::off_unchecked(x);
            Log::drop_on_commit(
                off,
                x.len() * mem::size_of::<T>().max(1),
                &*Journal::<Self>::current(true).unwrap().0,
            );
        }
    }

    /// Frees the allocation for value `x` immediately
    unsafe fn free_nolog<'a, T: ?Sized>(x: &T) {
        Self::perform(
            Self::pre_dealloc(x as *const _ as *mut u8, mem::size_of_val(x))
        );
    }

    /// Drops a `journal` from memory
    unsafe fn drop_journal(_journal: &mut Journal<Self>) where Self: MemPool { }

    /// Returns a reference to the offset of the first journal
    unsafe fn journals_head() -> &'static u64 { unimplemented!() }

    /// Runs a closure with a mutable reference to a thread->journal HashMap
    unsafe fn journals<T, F: Fn(&mut HashMap<ThreadId, (u64, i32)>)->T>(_: F)->T {
        unimplemented!()
    }

    /// Recovers from a crash
    unsafe fn recover() {
        unimplemented!()
    }

    /// Commits all changes and clears the logs for one thread
    ///
    /// If the transaction is nested, it postpones the commit to the top most
    /// transaction.
    ///
    /// # Safety
    ///
    /// This function is for internal use and should not be called elsewhere.
    ///
    #[inline]
    #[track_caller]
    unsafe fn commit() where Self: MemPool {
        // Self::discard(crate::ll::cpu());
        if let Some(journal) = Journal::<Self>::current(false) {
            *journal.1 -= 1;

            if *journal.1 == 0 {
                log!(Self, White, "COMMIT", "JRNL: {:?}", journal.0);

                let journal = as_mut(journal.0);
                journal.commit(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
                journal.clear(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
            }
        }
    }

    #[inline]
    /// Commits all changes without clearing the logs
    ///
    /// If the transaction is nested, it postpones the commit to the top most
    /// transaction.
    ///
    /// # Safety
    ///
    /// This function is for internal use and should not be called elsewhere.
    ///
    unsafe fn commit_no_clear() where Self: MemPool {
        // Self::discard(crate::ll::cpu());
        if let Some(journal) = Journal::<Self>::current(false) {
            *journal.1 -= 1;

            if *journal.1 == 0 {
                log!(Self, White, "COMMIT_NC", "JRNL: {:?}", journal.0);

                as_mut(journal.0).commit(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
            }
        }
    }

    #[inline]
    /// Clears the logs
    ///
    /// If the transaction is nested, it postpones the clear to the top most
    /// transaction.
    ///
    /// # Safety
    ///
    /// This function is for internal use and should not be called elsewhere.
    ///
    unsafe fn clear() where Self: MemPool {
        if let Some(journal) = Journal::<Self>::current(false) {
            *journal.1 -= 1;

            if *journal.1 == -1 {
                log!(Self, White, "CLEAR", "JRNL: {:?}", journal.0);

                as_mut(journal.0).clear(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
            }
        }
    }

    #[inline]
    /// Discards all changes and clears the logs
    ///
    /// If the transaction is nested, it propagates the panic up to the top most
    /// transaction to make all of them tainted. It returns true if it runs the
    /// rollback procedure; otherwise false.
    ///
    /// # Safety
    ///
    /// This function is for internal use and should not be called elsewhere.
    ///
    unsafe fn rollback() -> bool where Self: MemPool {
        // Self::discard(crate::ll::cpu());
        if let Some(journal) = Journal::<Self>::current(false) {
            *journal.1 -= 1;

            if *journal.1 == 0 {
                log!(Self, White, "ROLLBACK", "JRNL: {:?}", journal.0);

                let journal = as_mut(journal.0);
                journal.rollback(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
                journal.clear(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
                return true;
            } else {
                // Propagate the panic to the upper transactions
                panic!("Unsuccessful nested transaction");
            }
        }
        false
    }

    #[inline]
    /// Discards all changes without clearing the logs
    ///
    /// If the transaction is nested, it propagates the panic upto the top most
    /// transaction to make all of them tainted.
    ///
    /// # Safety
    ///
    /// This function is for internal use and should not be called elsewhere.
    ///
    unsafe fn rollback_no_clear() where Self: MemPool {
        if let Some(journal) = Journal::<Self>::current(false) {
            *journal.1 -= 1;

            if *journal.1 == 0 {
                log!(Self, White, "ROLLBACK_NC", "JRNL: {:?}", journal.0);

                as_mut(journal.0).rollback(
                    #[cfg(feature = "check_double_free")]
                    &mut *Self::dealloc_history()
                );
            }
        }
    }

    unsafe fn dealloc_history() -> *mut std::collections::HashSet<u64> {
        unimplemented!()
    }

    /// Executes commands atomically with respect to system crashes
    /// 
    /// The `transaction` function takes a closure with one argument of type
    /// `&Journal<Self>`. Before running the closure, it atomically creates a
    /// [`Journal`] object, if required, and prepares an immutable reference to
    /// it. Since there is no other safe way to create a `Journal` object, it
    /// ensures that every function taking an argument of type `&Journal<P>` is
    /// enforced to be invoked from a transaction.
    /// 
    /// The captured types are bounded to be [`TxInSafe`], unless explicitly
    /// asserted otherwise using [`AssertTxInSafe`] type wrapper. This
    /// guarantees the volatile state consistency, as well as the persistent
    /// state.
    /// 
    /// The returned type should be [`TxOutSafe`]. This prevents sending out
    /// unreachable persistent objects. The only way out of a transaction for
    /// a persistent object is to be reachable by the root object.
    ///
    /// # Examples
    /// 
    /// ```
    /// use corundum::default::*;
    /// 
    /// type P = Allocator;
    /// 
    /// let root = P::open::<PCell<i32>>("foo.pool", O_CF).unwrap();
    /// 
    /// let old = root.get();
    /// let new = Allocator::transaction(|j| {
    ///     root.set(root.get() + 1, j);
    ///     root.get()
    /// }).unwrap();
    /// 
    /// assert_eq!(new, old + 1);
    /// ```
    /// 
    /// [`Journal`]: ../stm/journal/struct.Journal.html
    /// [`TxInSafe`]: ../trait.TxInSafe.html
    /// [`TxOutSafe`]: ../trait.TxOutSafe.html
    /// [`AssertTxInSafe`]: ../struct.AssertTxInSafe.html
    /// 
    #[inline]
    #[track_caller]
    fn transaction<T, F: FnOnce(&'static Journal<Self>) -> T>(body: F) -> Result<T>
    where
        F: TxInSafe + UnwindSafe,
        T: TxOutSafe, Self: alloc::pool::MemPool
    {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Transaction;
        
        #[cfg(feature = "check_allocator_cyclic_links")]
        debug_assert!(Self::verify());

        let mut chaperoned = false;
        let cptr = &mut chaperoned as *mut bool;
        let res = std::panic::catch_unwind(|| {
            let chaperon = Chaperon::current();
            if let Some(ptr) = chaperon {
                // FIXME: Chaperone session is corrupted. fix it.
                unsafe {
                    *cptr = true;
                    let mut chaperon = &mut *ptr;
                    chaperon.postpone(
                        Self::commit_no_clear,
                        Self::rollback_no_clear,
                        Self::clear,
                    );
                    body({
                        #[cfg(feature = "stat_perf")]
                        let _perf = crate::stat::Measure::<Self>::Logging(std::time::Instant::now());
                        
                        let j = Journal::<Self>::current(true).unwrap();
                        *j.1 += 1;
                        let journal = as_mut(j.0);
                        journal.start_session(&mut chaperon);
                        journal.unset(JOURNAL_COMMITTED);
                        journal
                    })
                }
            } else {
                body({
                    #[cfg(feature = "stat_perf")]
                    let _perf = crate::stat::Measure::<Self>::Logging(std::time::Instant::now());

                    unsafe {
                        let j = Journal::<Self>::current(true).unwrap();
                        *j.1 += 1;
                        utils::as_mut(j.0).unset(JOURNAL_COMMITTED);
                        &*j.0
                    }
                })
            }
        });

        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<Self>::Logging(std::time::Instant::now());

        #[cfg(feature = "check_allocator_cyclic_links")]
        debug_assert!(Self::verify());

        unsafe {
            crate::ll::sfence();

            if let Ok(res) = res {
                if !chaperoned {
                    Self::commit();
                }
                Ok(res)
            } else {
                if !chaperoned {
                    Self::rollback();
                    Err("Unsuccessful transaction".to_string())
                } else {
                    // Propagates the panic to the top level in enforce rollback
                    panic!("Unsuccessful chaperoned transaction");
                }
            }
        }
    }

    fn gen() -> u32 {
        0
    }

    fn tx_gen() -> u32 {
        0
    }

    /// Prints memory information
    fn print_info() {}

    fn stat_footprint() -> usize {
        if cfg!(feature = "stat_footprint") {
            0
        } else {
            unimplemented!()
        }
    }
}

pub struct PoolGuard<P: MemPoolTraits>(pub PhantomData<P>);

impl<P: MemPoolTraits> PoolGuard<P> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<P: MemPoolTraits> Drop for PoolGuard<P> {
    fn drop(&mut self) {
        unsafe {
            P::close().unwrap();
        }

        crate::__cfg_stat_perf!({
            eprintln!("{}", crate::stat::report());
        });
    }
}

pub unsafe trait MemPool: 
    'static + 
    MemPoolTraits + 
    Sized + 
    Default + 
    Clone +
    Copy +
    PSafe + 
    TxInSafe + 
    LooseTxInUnsafe + 
    RefUnwindSafe + 
    UnwindSafe {}

pub(crate) fn create_file(filename: &str, size: u64) -> Result<()> {
    let file = OpenOptions::new().write(true).create(true).open(filename);
    if file.is_err() {
        Err(format!("{}", file.err().unwrap()))
    } else {
        if let Some(e) = file.unwrap().set_len(size).err() {
            Err(format!("{}", e))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::open_flags::*;
    use crate::alloc::pool::MemPoolTraits;
    use crate::default::*;

    #[test]
    #[ignore]
    fn nested_transactions() {
        let _image = Allocator::open_no_root("nosb.pool", O_CFNE);
        if let Err(e) = Allocator::transaction(|_| {
            let _ = Allocator::transaction(|_| {
                let _ = Allocator::transaction(|_| {
                    let _ = Allocator::transaction(|_| {
                        println!("should print");
                        panic!("intentional");
                    });
                    println!("should not print");
                });
                println!("should not print");
            });
            println!("should not print");
        }) {
            println!("Error: '{}'", e);
        }
    }
}
