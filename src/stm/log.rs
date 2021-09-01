use crate::ptr::Slice;
use crate::alloc::MemPool;
use crate::ll::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::*;
use std::clone::Clone;
use std::fmt::{self, Debug};
use std::ptr;

#[cfg(feature = "check_double_free")]
use std::collections::HashSet;

type Offset = u64;

/// Log Types
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum LogEnum {
    /// `(src, log, len)`: An undo log of slice `src..src+len` kept in
    /// `log..log+len`.
    DataLog(u64, u64, usize),

    /// `(u64, usize)`: Similar to [`DropOnFailure`] except that it
    /// drops the allocation when the high-level transaction is aborted. This is
    /// useful for temporarily unowned allocations, such as slices, because they
    /// are not deallocated via RAII.
    /// 
    /// [`DropOnFailure`]: ../alloc/trait.MemPool.html#method.drop_on_failure
    DropOnAbort(u64, usize),

    /// `(src, len)`: A drop log indicating that slice `src..src+len` should drop
    /// on commit, useful for in-transactional drop functions.
    DropOnCommit(u64, usize),

    /// `(src, len)`: A drop log indicating that slice `src..src+len` should drop
    /// on failure, useful for high-level allocation.
    DropOnFailure(u64, usize),

    /// `(src, inc/dec)`: A log indicating that the object is a counter
    /// and should increment/decrement on failure.
    RecountOnFailure(u64, bool),

    /// Unlocks a [`PMutex`](../sync/struct.PMutex.html) on transaction commit.
    UnlockOnCommit(u64),
    None,
}

fn offset_to_str(off: u64) -> String {
    if off == u64::MAX {
        "INF".to_string()
    } else {
        format!("{:x}", off)
    }
}

impl Debug for LogEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        match *self {
            DataLog(off, _, _)       => write!(f, "DataLog         ({})", offset_to_str(off)),
            DropOnAbort(off, _)      => write!(f, "DropOnAbort     ({})", offset_to_str(off)),
            DropOnCommit(off, _)     => write!(f, "DropOnCommit    ({})", offset_to_str(off)),
            DropOnFailure(off, _)    => write!(f, "DropOnFailure   ({})", offset_to_str(off)),
            RecountOnFailure(off, _) => write!(f, "RecountOnFailure({})", offset_to_str(off)),
            UnlockOnCommit(off)      => write!(f, "UnlockOnCommit  ({})", offset_to_str(off)),
            None                     => write!(f, "None"),
        }
    }
}

/// A data-log notification type
/// 
/// This is used to notify the owner that the underlying data is logged, so that
/// theres is no need for further log taking. This is done by updating a flag of
/// type `u8` which is a part of the owner's structure. The `Notifier` object
/// keeps a pointer to the flag and updates it accordingly. The pointer is
/// persistent meaning that it remains valid after restart of crash.
/// 
#[derive(PartialEq, Eq)]
pub enum Notifier<A: MemPool> {
    /// Atomically update the log flag
    Atomic(Ptr<u8, A>),
    /// Non-atomically update the flag
    NonAtomic(Ptr<u8, A>),
    /// There is no log flag
    None,
}

impl<A: MemPool> Copy for Notifier<A> {}

impl<A: MemPool> Clone for Notifier<A> {
    fn clone(&self) -> Self {
        use Notifier::*;
        match self {
            Atomic(c) => Atomic(c.clone()),
            NonAtomic(c) => NonAtomic(c.clone()),
            None => None,
        }
    }
}

impl<A: MemPool> Notifier<A> {

    #[inline]
    /// Update the owner's flag
    pub fn update(&mut self, v: u8) {
        use Notifier::*;
        match self {
            Atomic(n) => {
                if let Some(n) = n.as_option() {
                    unsafe { std::intrinsics::atomic_store_rel(n.as_mut_ptr(), v) }
                }
            }
            NonAtomic(n) => {
                if let Some(n) = n.as_option() {
                    unsafe {
                        *n.as_mut_ptr() = v;
                    }
                }
            }
            None => {}
        }
    }

    #[inline]
    /// Returns the offset of the log flag in the pool.
    /// 
    /// The notifier contains a pointer to the flag which is a part of the owner
    /// construct. If there is no specified flag, it returns `u64::MAX`.
    pub fn off(&self) -> u64 {
        use Notifier::*;
        match self {
            Atomic(n) => n.off(),
            NonAtomic(n) => n.off(),
            None => u64::MAX,
        }
    }
}

/// The `Log` type for pool `A`
/// 
/// It is pair of [`LogEnum`] and [`Notifier`] to keep a log in the [`Journal`].
/// A [`Journal`] comprises multiple pages with a fixed number of log slots.
/// Each slot can be filled by one `Log`. The [`Journal`] object uses these logs
/// to provide data consistency. Logs reside in the persistent region and their
/// durability is ensured by flushing the cache lines after each log.
/// 
/// The default mechanism of taking logs is copy-on-write which creates a log when
/// the object is mutably dereferenced. This requires two `clflush`es: one for
/// the log, and one for the update to the original data.
/// 
/// [`Journal`]: ./struct.Journal.html
/// [`LogEnum`]: ./enum.LogEnum.html
/// [`Notifier`]: ./enum.Notifier.html
/// 
pub struct Log<A: MemPool>(LogEnum, Notifier<A>);

impl<A: MemPool> Copy for Log<A> {}

impl<A: MemPool> Clone for Log<A> {
    fn clone(&self) -> Self {
        Self(self.0, self.1)
    }
}

impl<A: MemPool> PartialEq<LogEnum> for Log<A> {
    fn eq(&self, other: &LogEnum) -> bool { self.0 == *other }
}

impl<A: MemPool> PartialEq for Log<A> {
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 }
}

impl<A: MemPool> Debug for Log<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:?}", self.0)
    }
}

impl<A: MemPool> Default for Log<A> {
    #[inline]
    fn default() -> Self {
        Log(None, Notifier::None)
    }
}

impl<A: MemPool> Log<A> {
    /// Sets the `off` and `len` of the log
    /// 
    /// This function is used for low-level atomic allocation. The algorithm is
    /// as follows:
    /// 
    /// 1. Create a neutral drop log (`off = u64::MAX`) in a log slot in the [`Journal`]
    /// 2. Prepare allocation using [`pre_alloc()`]
    /// 3. Add a low-level log for updating the `off` and `len` of the drop log
    /// using `set` function
    /// 4. Perform the prepared changes to the allocator
    /// 
    /// Note that the deallocation of owned objects are handled through RAII.
    /// To reclaim the allocation of any sort on a failure, low-level
    /// `DropOnFailure` log is provided with the allocation.
    /// 
    /// If a crash happens before step 4, all changes are discarded and the
    /// drop log remains neutral. If a crash happens in the middle of step 4,
    /// the recovery procedure continues performing the changes, including the
    /// low-level logs for updating the drop log. Once it has the high-level
    /// drop log, the high-level recovery procedure reclaims the allocation as
    /// the crash happened inside a transaction.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # use corundum::stm::Log;
    /// # type P = Allocator;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// P::transaction(|j| unsafe {
    ///     // Create a neutral high-level log to drop the allocation on failure.
    ///     // It is different from the low-level drop log which is inside the
    ///     // allocator's ring buffer. Unlike that, this log is stored in the
    ///     // journal object.
    ///     let mut log = Log::drop_on_failure(u64::MAX, 1, j);
    ///    
    ///     // Prepare an allocation. The allocation is not durable yet. In case
    ///     // of a crash, the prepared allocated space is gone. It is fine
    ///     // because it has not been used. The `atomic_` functions
    ///     // form a low-level atomic section.
    ///     let (obj, off, len, zone) = P::atomic_new([1,2,3,4,5]);
    ///    
    ///     // Set the offset and size of the allocation to make the log valid.
    ///     // Note that the changes will be effective after the allocation is
    ///     // successfully performed.
    ///     log.set(off, len, zone);
    ///     
    ///     // It is fine to work with the prepared raw pointer. All changes in
    ///     // the low-level atomic section are considered as part of the
    ///     // allocation and will be gone in case of a crash, as the allocation
    ///     // will be dropped.
    ///     obj[1] = 20;
    ///    
    ///     // Transaction ends here. The perform function sets the `operating`
    ///     // flag to show that the prepared changes are being materialized.
    ///     // This flag remains set until the end of materialization. In case
    ///     // of a crash while operating, the recovery procedure first continues
    ///     // the materialization, and then uses the `DropOnFailure` logs to
    ///     // reclaim the allocation. `perform` function realizes the changes
    ///     // made by the `pre_` function on the given memory zone.
    ///     P::perform(zone);
    /// }).unwrap();
    /// ```
    /// 
    /// [`Journal`]: ./journal/struct.Journal.html
    /// [`pre_alloc()`]: ../alloc/trait.MemPool.html#method.pre_alloc
    /// [`validate()`]: ../alloc/trait.MemPool.html#method.validate
    pub fn set(&mut self, off: u64, len: usize, zone: usize) {
        debug_assert_ne!(len, 0);

        log!(A, Yellow, "CHNGE LOG", "TO:          ({:>6}:{:<6}) = {:<6} {:?}",
            offset_to_str(off), offset_to_str((off as usize + (len - 1)) as u64),
            len, self.0
        );

        match &self.0 {
            DropOnAbort(offset, length) |
            DropOnFailure(offset, length) |
            DropOnCommit(offset, length) => unsafe {
                A::log64(A::off_unchecked(offset), off, zone);
                A::log64(A::off_unchecked(length), len as u64, zone);
            },
            RecountOnFailure(offset, _) => unsafe {
                A::log64(A::off_unchecked(offset), off, zone);
            }
            _ => {}
        }
    }

    /// Returns an string specifying the type of this log
    pub fn kind(&self) -> String {
        match self.0 {
            DataLog(_, _, _) => "DataLog",
            DropOnAbort(_, _) => "DropOnAbort",
            DropOnCommit(_, _) => "DropOnCommit",
            DropOnFailure(_, _) => "DropOnFailure",
            RecountOnFailure(_, _) => "RecountOnFailure",
            UnlockOnCommit(_) => "UnlockOnCommit",
            None => "None"
        }.to_string()
    }

    /// Returns the inner value
    pub fn inner(&self) -> LogEnum {
        self.0
    }
}

use LogEnum::*;

#[cfg(feature = "verbose")]
fn dump_data<A: MemPool>(tag: &str, off: u64, len: usize) {
    use term_painter::Color::*;
    use term_painter::ToStyle;

    if *crate::utils::VERBOSE {
        print!("{:<8} {}", A::name().to_owned() + ":", 
            BrightBlue.paint(format!("{:>10}  ", tag)));
        for i in 0..len {
            let d = unsafe { A::get_unchecked::<u8>(off + i as u64) };
            print!("{}", BrightBlue.paint(format!("{:02x} ", *d)));
            if i % 16 == 15 && i+1 < len {
                println!();
                print!("{:>21}", " ");
            }
        }
    
        println!();
    }
}

impl<A: MemPool> Log<A> {
    /// Create a new log entry
    pub fn new(log: LogEnum, notifier: Notifier<A>) -> Self {
        Log(log, notifier)
    }

    #[inline]
    fn create_impl(
        off: u64,
        log: u64,
        len: usize,
        journal: &Journal<A>,
        notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);
        Self::write_on_journal(DataLog(off, log, len), journal, notifier)
    }

    /// Creates a log of `x` into `journal` and notifies the owner that log is
    /// created if `notifier` is specified.
    pub fn create<T: ?Sized>(
        x: &T,
        journal: &Journal<A>,
        mut notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::DataLog(std::time::Instant::now());

        let len = std::mem::size_of_val(x);
        if len == 0 {
            notifier.update(1);
            Ptr::dangling()
        } else {
            let pointer = unsafe { Ptr::<T, A>::new_unchecked(x) };

            log!(A, Yellow, "LOG", "FOR:         ({:>6}:{:<6}) = {:<6} DataLog  TYPE: {}",
                offset_to_str(pointer.off()), offset_to_str((pointer.off() as usize + (len - 1)) as u64),
                len, std::any::type_name_of_val(x)
            );
            #[cfg(feature = "verbose")] {
                dump_data::<A>("DATA", pointer.off(), len);
            }

            let log = unsafe { pointer.dup() };

            // if cfg!(feature = "replace_with_log") {
            //     pointer.replace(log.replace(pointer.off()));

            //     debug_assert_eq!(
            //         crate::utils::as_slice(pointer.as_ref()), 
            //         crate::utils::as_slice(log.as_ref()),
            //         "Log is not the same as the original data");

            //     Self::create_impl(log.off(), pointer.off(), len, journal, notifier)
            // } else {
            crate::ll::persist_obj_with_log::<_,A>(log.as_ref(), false);
            Self::create_impl(pointer.off(), log.off(), len, journal, notifier)
            // }
        }
    }

    /// Creates a log of `&[x]` into `journal` and notifies the owner that log is
    /// created if `notifier` is specified.
    pub fn create_slice<T: PSafe>(
        x: &[T],
        journal: &Journal<A>,
        mut notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::DataLog(std::time::Instant::now());

        let len = std::mem::size_of_val(x);
        if len == 0 {
            notifier.update(1);
            Ptr::dangling()
        } else {
            let slice = unsafe { Slice::<T, A>::new(x) };

            log!(A, Yellow, "LOG", "FOR:         ({:>6}:{:<6}) = {:<6} DataLog  TYPE: {}",
                offset_to_str(slice.off()), offset_to_str((slice.off() as usize + (len - 1)) as u64),
                len, std::any::type_name_of_val(x)
            );
            #[cfg(feature = "verbose")] {
                dump_data::<A>("DATA", slice.off(), len);
            }

            let log = unsafe { slice.dup() };

            crate::ll::persist_obj_with_log::<_,A>(log.as_ref(), false);
            Self::create_impl(slice.off(), log.off(), len, journal, notifier)
            // }
        }
    }

    /// Writes a `log` on a given `journal` and notifies the owner, if specified
    fn write_on_journal(
        log: LogEnum,
        journal: &Journal<A>,
        mut notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        let log = journal.write(log, notifier.clone());
        notifier.update(1);
        sfence();
        log
    }

    /// Creates a new [`DropOnCommit`](./enum.LogEnum.html#variant.DropOnCommit)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub fn drop_on_commit(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::DropLog(std::time::Instant::now());

        log!(A, Yellow, "NEW LOG", "FOR:         ({:>6}:{:<6}) = {:<6} DropOnCommit",
            offset_to_str(offset),
            offset_to_str((offset as usize + (len - 1)) as u64),
            len
        );
        Self::write_on_journal(DropOnCommit(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`DropOnAbort`](./enum.LogEnum.html#variant.DropOnAbort)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub fn drop_on_abort(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::DropLog(std::time::Instant::now());

        log!(A, Yellow, "NEW LOG", "FOR:         ({:>6}:{:<6}) = {:<6} DropOnAbort",
            offset_to_str(offset),
            offset_to_str((offset as usize + (len - 1)) as u64),
            len
        );

        Self::write_on_journal(DropOnAbort(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`DropOnFailure`](./enum.LogEnum.html#variant.DropOnFailure)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub unsafe fn drop_on_failure(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::DropLog(std::time::Instant::now());

        log!(A, Yellow, "NEW LOG", "FOR:         ({:>6}:{:<6}) = {:<6} DropOnFailure",
            offset_to_str(offset),
            offset_to_str((offset as usize + (len - 1)) as u64),
            len
        );

        Self::write_on_journal(DropOnFailure(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`UnlockOnCommit`](./enum.LogEnum.html#variant.UnlockOnCommit)
    /// for locking data in a thread
    #[inline]
    #[track_caller]
    pub unsafe fn unlock_on_commit(
        virt_addr: u64,
        journal: &Journal<A>,
    ) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::MutexLog(std::time::Instant::now());

        log!(A, Yellow, "NEW LOG", "FOR:         v@{:<18} UnlockOnCommit", virt_addr);
        
        #[cfg(any(feature = "no_pthread", windows))] {
            let b = &mut *(virt_addr as *mut (bool, u64));
            if b.0 { return; }
        }
        #[cfg(not(any(feature = "no_pthread", windows)))] {
            let b = &mut *(virt_addr as *mut (bool, libc::pthread_mutex_t, 
                libc::pthread_mutexattr_t));
            if b.0 { return; }
        };

        Self::write_on_journal(UnlockOnCommit(virt_addr), journal, Notifier::None);
    }

    /// Creates a new [`RecountOnFailure`](./enum.LogEnum.html#variant.RecountOnFailure)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub unsafe fn recount_on_failure(offset: u64, inc: bool, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        log!(A, Yellow, "NEW LOG", "FOR:         ({:>6}:{:<6}) = {:<6} RecountOnFailure",
            offset_to_str(offset),
            offset_to_str(offset),
            8
        );
        Self::write_on_journal(RecountOnFailure(offset, inc), journal, Notifier::None)
    }

    fn rollback_datalog(src: &mut u64, log: &mut u64, len: &usize) {
        debug_assert_ne!(*len, 0);

        if *log != u64::MAX && *src != u64::MAX {
            log!(A, Magenta, "ROLLBACK", "FOR:         ({:>6x}:{:<6x}) = {:<6} DataLog({})",
                *src, *src as usize + (len - 1), len, log   
            );
            #[cfg(feature = "verbose")] {
                dump_data::<A>(" ORG", *src, *len);
                dump_data::<A>(" LOG", *log, *len);
            } 
            unsafe {
                let src = A::get_mut_unchecked::<u8>(*src);
                let log = A::get_mut_unchecked::<u8>(*log);
                ptr::copy_nonoverlapping(log, src, *len);
                persist_with_log::<_,A>(log, *len, false);
            }
                    
            #[cfg(feature = "check_allocator_cyclic_links")]
            debug_assert!(A::verify());
        }
    }

    pub(crate) unsafe fn rollback(&mut self) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::RollbackLog(std::time::Instant::now());

        match &mut self.0 {
            DataLog(src, log, len) => {
                Self::rollback_datalog(src, log, len);
                self.notify(0);
                self.1 = Notifier::None;
                    
                #[cfg(feature = "check_allocator_cyclic_links")]
                debug_assert!(A::verify());
            }
            _ => {}
        }
    }

    pub(crate) unsafe fn rollback_drop_on_abort(&mut self, 
        #[cfg(feature = "check_double_free")]
        check_double_free: &mut HashSet<u64>
    ) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::RollbackLog(std::time::Instant::now());

        match &mut self.0 {
            DropOnAbort(src, len) => {
                if *src != u64::MAX {
                    #[cfg(feature = "check_double_free")] {
                        if check_double_free.contains(&*src) {
                            return;
                        }
                        check_double_free.insert(*src);
                    }
                    let z = A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                    A::log64(A::off_unchecked(src), u64::MAX, z);
                    A::perform(z);
                    
                    #[cfg(feature = "check_allocator_cyclic_links")]
                    debug_assert!(A::verify());
                }
            }
            _ => {}
        }
    }

    /// Recovers from the crash or power failure
    pub(crate) unsafe fn recover(&mut self, rollback: bool, 
        #[cfg(feature = "check_double_free")]
        check_double_free: &mut HashSet<u64>
    ) {
        match &mut self.0 {
            DataLog(src, log, layout) => {
                if rollback {
                    debug_assert!(A::allocated(*src, 1), "Access Violation at address 0x{:x}", *src);
                    debug_assert!(A::allocated(*log, 1), "Access Violation at address 0x{:x}", *log);
                    Self::rollback_datalog(src, log, layout);
                    self.notify(0);
                    self.1 = Notifier::None;
                    
                    #[cfg(feature = "check_allocator_cyclic_links")]
                    debug_assert!(A::verify());
                }
            }
            DropOnFailure(src, len) => {
                if rollback {
                    if *src != u64::MAX {
                        #[cfg(feature = "check_double_free")] {
                            if check_double_free.contains(&*src) {
                                return;
                            }
                            check_double_free.insert(*src);
                        }
                        debug_assert!(A::allocated(*src, 1), "Access Violation (0x{:x})", *src);
                        let z = A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                        A::log64(A::off_unchecked(src), u64::MAX, z);
                        A::perform(z);
                    
                        #[cfg(feature = "check_allocator_cyclic_links")]
                        debug_assert!(A::verify());
                    }
                }
            }
            RecountOnFailure(src, inc) => {
                let off = *src;
                if off != u64::MAX {
                    debug_assert!(A::allocated(off, 1), "Access Violation (0x{:x}))", off);
                    let c = A::get_mut_unchecked::<u64>(off);
                    let z = A::zone(off);
                    A::prepare(z);
                    if *c != u64::MAX {
                        if *inc {
                            A::log64(off, *c as u64 + 1, z);
                        } else {
                            A::log64(off, *c as u64 - 1, z);
                        }
                    }
                    A::log64(A::off_unchecked(src), u64::MAX, z);
                    A::perform(z);
                    
                    #[cfg(feature = "check_allocator_cyclic_links")]
                    debug_assert!(A::verify());
                }
            }
            UnlockOnCommit(src) => {
                *src = u64::MAX;
            }
            _ => {}
        }
    }

    /// Commits changes
    pub(crate) fn commit_data(&mut self) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::CommitLog(std::time::Instant::now());

        match &mut self.0 {
            DataLog(_src, _log, _len) => {
                debug_assert!(A::allocated(*_src, 1), "Access Violation at address 0x{:x}", *_src);

                #[cfg(all(not(feature = "no_flush_updates"), not(feature = "replace_with_log")))]
                unsafe {
                    persist_with_log::<u8,A>(A::get_mut_unchecked(*_src), *_len, false);
                }
            }
            _ => {}
        }
    }

    /// Commits changes
    pub(crate) fn commit_dealloc(&mut self, 
        #[cfg(feature = "check_double_free")]
        check_double_free: &mut HashSet<u64>
    ) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::CommitLog(std::time::Instant::now());

        match &mut self.0 {
            DropOnCommit(src, len) => {
                if *src != u64::MAX {
                    unsafe {
                        #[cfg(feature = "check_double_free")] {
                            if check_double_free.contains(&*src) {
                                return;
                            }
                            check_double_free.insert(*src);
                        }
                        let z = A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                        A::log64(A::off_unchecked(src), u64::MAX, z);
                        A::perform(z);
                    
                        #[cfg(feature = "check_allocator_cyclic_links")]
                        debug_assert!(A::verify());
                    }
                }
            }
            _ => {}
        }
    }

    /// Clears this log and notifies the owner
    /// 
    /// * If it is a [`DataLog`](./enum.LogEnum.html#variant.DataLog), it reclaims
    /// the allocation for the log.
    /// * If it is a [`UnlockOnCommit`](./enum.LogEnum.html#variant.UnlockOnCommit),
    /// it unlocks the mutex.
    /// 
    pub unsafe fn clear(&mut self, 
        #[cfg(feature = "check_double_free")]
        check_double_free: &mut HashSet<u64>
    ) {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::ClearLog(std::time::Instant::now());

        match &mut self.0 {
            DataLog(_src, log, len) => {
                if *log != u64::MAX {
                    #[cfg(feature = "check_double_free")] {
                        if check_double_free.contains(&*log) {
                            return;
                        }
                        check_double_free.insert(*log);
                    }
                    log!(A, Magenta, "DEL LOG", "FOR:         ({:>6x}:{:<6x}) = {:<6} DataLog({})",
                        *_src, *_src as usize + (*len - 1), *len, log
                    );
                    debug_assert!(A::allocated(*log, *len), "Access Violation at address 0x{:x}", *log);

                    #[cfg(feature = "check_allocator_cyclic_links")]
                    debug_assert!(A::verify());

                    let z = A::pre_dealloc(A::get_mut_unchecked(*log), *len);
                    A::log64(A::off_unchecked(log), u64::MAX, z);
                    A::perform(z);

                    #[cfg(feature = "check_allocator_cyclic_links")]
                    debug_assert!(A::verify());
                }
            }
            UnlockOnCommit(src) => {
                if *src != u64::MAX {
                    log!(A, Magenta, "UNLOCK", "FOR:          v@{}", *src);
                    #[cfg(not(any(feature = "no_pthread", windows)))] {
                        let b = &mut *(*src as *mut (bool, libc::pthread_mutex_t, libc::pthread_mutexattr_t));
                        b.0 = false;
                        let lock = &mut b.1;
                        let attr = &mut b.2;
                        let result = libc::pthread_mutex_unlock(lock);
                        if result != 0 {
                            crate::sync::init_lock(lock, attr);
                        }
                    }
                    #[cfg(any(feature = "no_pthread", windows))] {
                        let b = &mut *(*src as *mut (bool, u64));
                        b.0 = false;
                        let lock = &mut b.1;
                        std::intrinsics::atomic_store_rel(lock, 0);
                    }

                    *src = u64::MAX;
                }
            }
            _ => {}
        }
    }

    /// Notify the owner that the log is created/cleared according to `v`
    #[inline]
    pub unsafe fn notify(&mut self, v: u8) {
        if let DataLog(src, _, _) = self.0 {
            if src != u64::MAX {
                self.1.update(v);
            }
        }
    }
}

/// A generic trait for taking a log of any type
pub trait Logger<A: MemPool> {
    /// Creates a log of `self` and update the log flag if specified in `notifier`
    unsafe fn create_log(&self, journal: &Journal<A>, notifier: Notifier<A>) -> Ptr<Log<A>, A>;
}

impl<T: PSafe + ?Sized, A: MemPool> Logger<A> for T {
    default unsafe fn create_log(&self, journal: &Journal<A>, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        Log::create(self, journal, notifier)
    }
}

impl<T: PSafe, A: MemPool> Logger<A> for [T] {
    unsafe fn create_log(&self, journal: &Journal<A>, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        Log::create_slice(self, journal, notifier)
    }
}
