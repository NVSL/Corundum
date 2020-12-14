use crate::alloc::MemPool;
use crate::ll::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::PSafe;
use std::clone::Clone;
use std::fmt::{self, Debug};
use std::ptr;

#[cfg(feature = "verbose")]
use term_painter::Color::*;

#[cfg(feature = "verbose")]
use term_painter::ToStyle;

type Offset = u64;

/// Log Types
#[derive(Copy, Clone)]
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

    /// Unlocks a [`Mutex`](../sync/struct.Mutex.html) on transaction commit.
    UnlockOnCommit(u64),
    None,
}

fn offset_to_str(off: u64) -> String {
    if off == u64::MAX {
        "INF".to_string()
    } else {
        off.to_string()
    }
}

impl Debug for LogEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        match *self {
            DataLog(off, _, _) => write!(f, "DataLog({})", offset_to_str(off)),
            DropOnAbort(off, _) => write!(f, "DropOnAbort({})", offset_to_str(off)),
            DropOnCommit(off, _) => write!(f, "DropOnCommit({})", offset_to_str(off)),
            DropOnFailure(off, _) => write!(f, "DropOnFailure({})", offset_to_str(off)),
            UnlockOnCommit(off) => write!(f, "UnlockOnCommit({})", offset_to_str(off)),
            None => write!(f, "None"),
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
/// The default mechanism of taking logs is copy-on-write which takes a log when
/// the object is mutably dereferenced. This requires two `clflush`es: one for
/// the log, and one for the update to the original data.
/// 
/// [`Journal`]: ./struct.Journal.html
/// [`LogEnum`]: ./enum.LogEnum.html
/// [`Notifier`]: ./enum.Notifier.html
pub struct Log<A: MemPool>(LogEnum, Notifier<A>);

impl<A: MemPool> Copy for Log<A> {}

impl<A: MemPool> Clone for Log<A> {
    fn clone(&self) -> Self {
        Self(self.0, self.1)
    }
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
    /// 1. Take a neutral drop log (`off = u64::MAX`) in a log slot in the [`Journal`]
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
    /// [`Journal`]: ./journal/struct.Journal.html
    /// [`pre_alloc()`]: ../alloc/trait.MemPool.html#method.pre_alloc
    /// [`validate()`]: ../alloc/trait.MemPool.html#method.validate
    pub fn set(&mut self, off: u64, len: usize) {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "verbose")]
        println!(
            "{}",
            Yellow.paint(format!(
                "        CHNGE LOG     TO:         ({:>4}..{:<4}) = {:<5} {:?}",
                offset_to_str(off),
                offset_to_str((off as usize + (len - 1)) as u64),
                len,
                self.0
            ))
        );

        match &self.0 {
            DropOnAbort(offset, length) |
            DropOnFailure(offset, length) |
            DropOnCommit(offset, length) => unsafe {
                A::log64(offset, off);
                A::log64(length as *const usize as *const u64, len as u64);
            },
            _ => {}
        }
    }
}

use LogEnum::*;

/// Print traces verbosely
#[allow(unused_macros)]
macro_rules! print_log {
    ($x:expr,$off:expr,$len:expr) => {
        #[cfg(feature = "verbose")] {
            println!(
                "{}",
                Yellow.paint(format!(
                    "              LOG    FOR:         ({:>4}..{:<4}) = {:<5} DataLog  TYPE: {}",
                    offset_to_str($off),
                    offset_to_str(($off as usize + ($len - 1)) as u64),
                    $len,
                    std::any::type_name_of_val($x)
                ))
            );
            dump_data::<A>("DATA", $off, $len);
        }
    };
}

#[cfg(feature = "verbose")]
fn dump_data<A: MemPool>(tag: &str, off: u64, len: usize) {
    print!("{}", BrightBlue.paint(format!("             {}    ", tag)));
    for i in 0..len {
        let d = unsafe { A::get_unchecked::<u8>(off + i as u64) };
        print!("{}", BrightBlue.paint(format!("{:02x} ", *d)));
        if i % 16 == 15 && i+1 < len {
            println!();
            print!("                     ");
        }
    }
    println!();
}

impl<A: MemPool> Log<A> {
    /// Create a new log entry
    pub fn new(log: LogEnum, notifier: Notifier<A>) -> Self {
        Log(log, notifier)
    }

    #[inline]
    #[cfg(feature = "replace_with_log")]
    fn take_impl(
        off: &u64,
        log: &u64,
        len: usize,
        journal: &Journal<A>,
        notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);
        let res = Self::write_on_journal(DataLog(*off, *log, len), journal, notifier);
        unsafe {
            let tmp = *off;
            *(off as *const u64 as *mut u64) = *log;
            *(log as *const u64 as *mut u64) = tmp;
            msync(off, 1);
        }
        res
    }


    #[inline]
    #[cfg(not(feature = "replace_with_log"))]
    fn take_impl(
        off: u64,
        log: u64,
        len: usize,
        journal: &Journal<A>,
        notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);
        Self::write_on_journal(DataLog(off, log, len), journal, notifier)
    }

    /// Takes a log of `x` into `journal` and notifies the owner that log is
    /// taken if `notifier` is specified.
    pub fn take<T: PSafe + ?Sized>(
        x: &T,
        journal: &Journal<A>,
        notifier: Notifier<A>,
    ) -> Ptr<Log<A>, A> {
        let len = std::mem::size_of_val(x);
        if len == 0 {
            Ptr::dangling()
        } else {
            let pointer = unsafe { Ptr::<T, A>::new_unchecked(x) };

            #[cfg(feature = "verbose")]
            print_log!(x, pointer.off(), len);

            #[cfg(not(feature = "replace_with_log"))]
            unsafe { Self::take_impl(pointer.off(), pointer.dup().off(), len, journal, notifier) }
            
            #[cfg(feature = "replace_with_log")]
            unsafe { Self::take_impl(pointer.off_ref(), pointer.dup().off_ref(), len, journal, notifier) }
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
        log
    }

    /// Creates a new [`DropOnCommit`](./enum.LogEnum.html#variant.DropOnCommit)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub fn drop_on_commit(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "verbose")]
        println!(
            "{}",
            Yellow.paint(format!(
                "          NEW LOG    FOR:         ({:>4}..{:<4}) = {:<5} DropOnCommit",
                offset_to_str(offset),
                offset_to_str((offset as usize + (len - 1)) as u64),
                len
            ))
        );
        Self::write_on_journal(DropOnCommit(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`DropOnAbort`](./enum.LogEnum.html#variant.DropOnAbort)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub fn drop_on_abort(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "verbose")]
        println!(
            "{}",
            Yellow.paint(format!(
                "          NEW LOG    FOR:         ({:>4}..{:<4}) = {:<5} DropOnAbort",
                offset_to_str(offset),
                offset_to_str((offset as usize + (len - 1)) as u64),
                len
            ))
        );
        Self::write_on_journal(DropOnAbort(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`DropOnFailure`](./enum.LogEnum.html#variant.DropOnFailure)
    /// log and writes it on `journal`
    #[inline]
    #[track_caller]
    pub fn drop_on_failure(offset: u64, len: usize, journal: &Journal<A>) -> Ptr<Log<A>, A> {
        debug_assert_ne!(len, 0);

        #[cfg(feature = "verbose")]
        println!(
            "{}",
            Yellow.paint(format!(
                "          NEW LOG    FOR:         ({:>4}..{:<4}) = {:<5} DropOnFailure",
                offset_to_str(offset),
                offset_to_str((offset as usize + (len - 1)) as u64),
                len
            ))
        );

        Self::write_on_journal(DropOnFailure(offset, len), journal, Notifier::None)
    }

    /// Creates a new [`UnlockOnCommit`](./enum.LogEnum.html#variant.UnlockOnCommit)
    /// for locking data in a thread
    #[inline]
    #[track_caller]
    pub fn unlock_on_commit(
        virt_addr: u64,
        journal: &Journal<A>,
    ) {
        #[cfg(feature = "verbose")]
        {
            println!(
                "{}",
                Yellow.paint(format!(
                    "          NEW LOG    FOR:         v@{:<18} UnlockOnCommit",
                    virt_addr
                ))
            );
        }
        
        if cfg!(feature = "pthread") {
            unsafe {
                let b = &mut *(virt_addr as *mut (bool, libc::pthread_mutex_t, 
                    libc::pthread_mutexattr_t));
                if b.0 { return; }
            }
        } else {
            unsafe {
                let b = &mut *(virt_addr as *mut (bool, u64));
                if b.0 { return; }
            }
        };

        Self::write_on_journal(UnlockOnCommit(virt_addr), journal, Notifier::None);
    }

    fn rollback_datalog(src: &mut u64, log: &mut u64, len: &usize) {
        debug_assert_ne!(*len, 0);

        if *log != u64::MAX && *src != u64::MAX {
            #[cfg(feature = "verbose")] {
                println!(
                    "{}",
                    Magenta.paint(format!(
                        "         ROLLBACK    FOR:         ({:>4}..{:<4}) = {:<5} DataLog({})",
                        *src,
                        *src as usize + (len - 1),
                        len,
                        log
                    ))
                );
                dump_data::<A>(" ORG", *src, *len);
                dump_data::<A>(" LOG", *log, *len);
            }
            unsafe {
                let src = A::get_mut_unchecked::<u8>(*src);
                let log = A::get_mut_unchecked::<u8>(*log);
                ptr::copy_nonoverlapping(log, src, *len);
                msync(log, *len);
            }
        }
    }

    pub(crate) fn rollback(&mut self) {
        match &mut self.0 {
            DataLog(src, log, len) => {
                Self::rollback_datalog(src, log, len);
                self.notify(0);
                self.1 = Notifier::None;
            }
            DropOnAbort(src, len) => {
                if *src != u64::MAX {
                    unsafe {
                        A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                        A::log64(src, u64::MAX);
                        A::perform();
                    }
                }
            }
            _ => {}
        }
    }

    /// Recovers from the crash or power failure
    pub(crate) fn recover(&mut self, rollback: bool) {
        match &mut self.0 {
            DataLog(src, log, layout) => {
                if rollback {
                    debug_assert!(A::allocated(*src, 1), "Access Violation at address 0x{:x}", *src);
                    debug_assert!(A::allocated(*log, 1), "Access Violation at address 0x{:x}", *log);
                    Self::rollback_datalog(src, log, layout);
                    self.notify(0);
                    self.1 = Notifier::None;
                }
            }
            DropOnFailure(src, len) => {
                if rollback {
                    if *src != u64::MAX {
                        unsafe {
                            debug_assert!(A::allocated(*src, 1), "Access Violation");
                            A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                            *src = u64::MAX;
                            A::perform();
                        }
                    }
                }
            }
            UnlockOnCommit(src) => {
                *src = u64::MAX;
            }
            _ => {}
        }
    }

    /// Commits changes
    pub(crate) fn commit(&mut self) {
        match &mut self.0 {
            DataLog(_src, _log, _len) => {
                debug_assert!(A::allocated(*_src, 1), "Access Violation at address 0x{:x}", *_src);

                #[cfg(all(not(feature = "no_flush_updates"), not(feature = "replace_with_log")))]
                unsafe {
                    msync::<u8>(A::get_mut_unchecked(*_src), *_len);
                }
            }
            DropOnCommit(src, len) => {
                if *src != u64::MAX {
                    unsafe {
                        A::pre_dealloc(A::get_mut_unchecked(*src), *len);
                        A::log64(src, u64::MAX);
                        A::perform();
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
    pub fn clear(&mut self) {
        match &mut self.0 {
            DataLog(_src, log, len) => {
                if *log != u64::MAX {
                    #[cfg(feature = "verbose")]
                    println!(
                        "{}",
                        Magenta.paint(format!(
                            "          DEL LOG    FOR:         ({:>4}..{:<4}) = {:<5} DataLog({})",
                            *_src,
                            *_src as usize + (*len - 1),
                            *len,
                            log
                        ))
                    );
                    unsafe {
                        debug_assert!(A::allocated(*log, 1), "Access Violation at address 0x{:x}", *log);
                        A::pre_dealloc(A::get_mut_unchecked(*log), *len);
                        A::log64(log, u64::MAX);
                        A::perform();
                    }
                }
            }
            UnlockOnCommit(src) => {
                if *src != u64::MAX {
                    #[cfg(feature = "verbose")]
                    {
                        println!(
                            "{}",
                            Magenta
                                .paint(format!("           UNLOCK    FOR:          v@{}", *src,))
                        );
                    }
                    unsafe {
                        #[cfg(feature = "pthread")] {
                            let b = &mut *(*src as *mut (bool, libc::pthread_mutex_t, libc::pthread_mutexattr_t));
                            b.0 = false;
                            let lock = &mut b.1;
                            let attr = &mut b.2;
                            let result = libc::pthread_mutex_unlock(lock);
                            if result != 0 {
                                crate::sync::init_lock(lock, attr);
                            }
                        }
                        #[cfg(not(feature = "pthread"))] {
                            let b = &mut *(*src as *mut (bool, u64));
                            b.0 = false;
                            let lock = &mut b.1;
                            std::intrinsics::atomic_store_rel(lock, 0);
                        }

                        *src = u64::MAX;
                    }
                }
            }
            _ => {}
        }
    }

    /// Notify the owner that the log is taken/cleared according to `v`
    #[inline]
    pub fn notify(&mut self, v: u8) {
        if let DataLog(src, _, _) = self.0 {
            if src != u64::MAX {
                self.1.update(v)
            }
        }
    }
}

/// A generic trait for taking a log of any type
pub trait Logger<A: MemPool> {
    /// Takes a log of `self` and update the log flag if specified in `notifier`
    unsafe fn take_log(&self, journal: &Journal<A>, notifier: Notifier<A>) -> Ptr<Log<A>, A>;
}

impl<T: PSafe + ?Sized, A: MemPool> Logger<A> for T {
    unsafe fn take_log(&self, journal: &Journal<A>, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        Log::take(self, journal, notifier)
    }
}
