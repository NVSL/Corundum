use crate::alloc::MemPool;
use crate::cell::VCell;
use crate::ptr::Ptr;
use crate::stm::{Journal, Log, Notifier, Logger};
use crate::*;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::{TryLockError, TryLockResult};

#[allow(unused_imports)]
use std::{fmt, intrinsics};

/// A transaction-wide recursive mutual exclusion primitive useful for
/// protecting shared data while transaction is open. Further locking in the
/// same thread is non-blocking. Any access to data is serialized. Borrow rules
/// are checked dynamically to prevent multiple mutable dereferencing.
///
/// This mutex will block threads/transactions waiting for the lock to become
/// available. The difference between `Mutex` and [`std`]`::`[`sync`]`::`[`Mutex`]
/// is that it will hold the lock until the transaction commits. For example,
/// consider the following code snippet in which a shared object is protected
/// with [`std`]`::`[`sync`]`::`[`Mutex`]. In this case, data might be lost.
///
/// ```no_compile
/// use corundum::default::*;
/// use std::sync::Mutex;
/// 
/// type P = BuddyAlloc;
/// 
/// let obj = P::open::<Parc<Mutex<i32>>>("foo.pool", O_CF).unwrap();
/// //                       ^ std::sync::Mutex is not PSafe
///
/// transaction(|j| {
///     {
///         let obj = obj.lock().unwrap();
///         // Some statements ...
///     } // <-- release the lock here
///
///     // Another thread can work with obj
///
///     {
///         let obj = obj.lock().unwrap();
///         // Some statements ...
///     } // <-- release the lock here
///     
///     // A crash may happen here after another thread has used updated data
///     // which leads to an inconsistent state
/// });
/// ```
///
/// The safest way to have a shared object protected from both data-race and
/// data-loss is to wrap it with a transaction-wide `Mutex` as in the following
/// example:
///
/// ```
/// use corundum::default::*;
/// 
/// type P = BuddyAlloc;
/// 
/// // PMutex<T> = corundum::sync::Mutex<T,P>
/// let obj = P::open::<Parc<PMutex<i32>>>("foo.pool", O_CF).unwrap();
///
/// transaction(|j| {
///     {
///         let obj = obj.lock(j);
///         // Some statements ...
///     }
///
///     // data is still locked.
///
///     {
///         let obj = obj.lock(j); // <-- does not block the current thread
///         // Some statements ...
///     }
///     
/// }); // <-- release the lock here after committing or rolling back the transaction
/// ```
///
/// [`new`]: #method.new
/// [`lock`]: #method.lock
/// [`Mutex`]: std::sync::Mutex
/// [`sync`]: std::sync
/// [`std`]: std
///
pub struct Mutex<T, A: MemPool> {
    heap: PhantomData<A>,
    inner: VCell<MutexInner, A>,
    data: UnsafeCell<(u8, T)>,
}

struct MutexInner {
    borrowed: bool,

    #[cfg(not(any(feature = "no_pthread", windows)))]
    lock: (bool, libc::pthread_mutex_t, libc::pthread_mutexattr_t),

    #[cfg(any(feature = "no_pthread", windows))]
    lock: (bool, u64)
}

impl Default for MutexInner {

    #[cfg(not(any(feature = "no_pthread", windows)))]
    fn default() -> Self {
        use std::mem::MaybeUninit;
        let mut attr = MaybeUninit::<libc::pthread_mutexattr_t>::uninit();
        let mut lock = libc::PTHREAD_MUTEX_INITIALIZER;
        unsafe { init_lock(&mut lock, attr.as_mut_ptr()); }
        MutexInner { borrowed: false, lock: (false, lock, unsafe { attr.assume_init() }) }
    }

    #[cfg(any(feature = "no_pthread", windows))]
    fn default() -> Self {
        MutexInner { borrowed: false, lock: (false, 0) }
    }
}

impl MutexInner {
    fn acquire(&self) -> bool {
        if self.borrowed {
            false
        } else {
            as_mut(self).borrowed = true;
            true
        }
    }

    fn release(&self) {
        as_mut(self).borrowed = false;
    }
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Mutex<T, A> {}
impl<T, A: MemPool> UnwindSafe for Mutex<T, A> {}
impl<T, A: MemPool> RefUnwindSafe for Mutex<T, A> {}

unsafe impl<T, A: MemPool> TxInSafe for Mutex<T, A> {}
unsafe impl<T, A: MemPool> PSafe for Mutex<T, A> {}
unsafe impl<T: Send, A: MemPool> Send for Mutex<T, A> {}
unsafe impl<T: Send, A: MemPool> Sync for Mutex<T, A> {}
unsafe impl<T, A: MemPool> PSend for Mutex<T, A> {}

impl<T, A: MemPool> Mutex<T, A> {
    /// Creates a new `Mutex`
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::alloc::*;
    /// use corundum::sync::{Parc,Mutex};
    /// 
    /// Heap::transaction(|j| {
    ///     let p = Parc::new(Mutex::new(10, j), j);
    /// }).unwrap();
    /// ```
    pub fn new(data: T, _journal: &Journal<A>) -> Mutex<T, A> {
        Mutex {
            heap: PhantomData,
            inner: VCell::new(MutexInner::default()),
            data: UnsafeCell::new((0, data)),
        }
    }
}

impl<T: PSafe, A: MemPool> Mutex<T, A> {
    #[inline]
    #[allow(clippy::mut_from_ref)]
    /// Takes a log and returns a `&mut T` for interior mutability
    pub(crate) fn get_mut(&self, journal: &Journal<A>) -> &mut T {
        unsafe {
            let inner = &mut *self.data.get();
            if inner.0 == 0 {
                inner.1.take_log(journal, Notifier::NonAtomic(Ptr::from_ref(&inner.0)));
            }
            &mut inner.1
        }
    }

    #[inline]
    #[allow(clippy::mut_from_ref)]
    fn self_mut(&self) -> &mut Self {
        unsafe {
            let ptr: *const Self = self;
            &mut *(ptr as *mut Self)
        }
    }
}

impl<T, A: MemPool> Mutex<T, A> {
    #[inline]
    fn raw_lock(&self, journal: &Journal<A>) {
        unsafe {
            // Log::unlock_on_failure(self.inner.get(), journal);
            let lock = &self.inner.lock.1 as *const _ as *mut _;
            #[cfg(not(any(feature = "no_pthread", windows)))] {
                libc::pthread_mutex_lock(lock);
            }
            #[cfg(any(feature = "no_pthread", windows))] {
                let tid = std::thread::current().id().as_u64().get();
                while intrinsics::atomic_cxchg_acqrel(lock, 0, tid).0 != tid {}
            }
            if self.inner.acquire() {
                Log::unlock_on_commit(&self.inner.lock as *const _ as u64, journal);
            } else {
                #[cfg(not(any(feature = "no_pthread", windows)))]
                libc::pthread_mutex_unlock(lock);

                #[cfg(any(feature = "no_pthread", windows))] 
                intrinsics::atomic_store_rel(lock, 0);

                panic!("Cannot have multiple instances of MutexGuard");
            }
        }
    }

    /// Acquires a mutex, blocking the current thread until it is able to do so.
    /// 
    /// This function will block the local thread until it is available to
    /// acquire the mutex. Upon returning, the thread is the only thread with
    /// the lock held. An RAII guard is returned to keep track of borrowing
    /// data. It creates an [`UnlockOnCommit`] log to unlock the mutex when
    /// transaction is done.
    /// 
    /// If the local thread already holds the lock, `lock()` does not block it.
    /// The mutex remains locked until the transaction is committed. 
    /// Alternatively, [`PMutex`] can be used as a compact form of `Mutex`.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use corundum::default::*;
    /// use corundum::sync::{Parc,Mutex};
    /// use std::thread;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let obj = P::open::<Parc<Mutex<i32,P>,P>>("foo.pool", O_CF).unwrap();
    /// 
    /// // Using short forms in the pool module, there is no need to specify the
    /// // pool type, as follows:
    /// // let obj = P::open::<Parc<PMutex<i32>>>("foo.pool", O_CF).unwrap();
    /// 
    /// let obj = Parc::demote(&obj);
    /// thread::spawn(move || {
    ///     transaction(move |j| {
    ///         if let Some(obj) = obj.promote(j) {
    ///             *obj.lock(j) += 1;
    ///         }
    ///     }).unwrap();
    /// }).join().expect("thread::spawn failed");
    /// ```
    /// 
    /// [`PMutex`]: ../default/type.PMutex.html
    /// [`UnlockOnCommit`]: ../stm/enum.LogEnum.html#variant.UnlockOnCommit
    /// 
    pub fn lock<'a>(&'a self, journal: &'a Journal<A>) -> MutexGuard<'a, T, A> {
        self.raw_lock(journal);
        unsafe { MutexGuard::new(self, journal) }
    }

    #[inline]
    fn raw_trylock(&self, journal: &Journal<A>) -> bool {
        unsafe {
            let lock = &self.inner.lock.1 as *const _ as *mut _;

            #[cfg(not(any(feature = "no_pthread", windows)))]
            let result = libc::pthread_mutex_trylock(lock) == 0;

            #[cfg(any(feature = "no_pthread", windows))]
            let result = {
                let tid = std::thread::current().id().as_u64().get();
                intrinsics::atomic_cxchg_acqrel(lock, 0, tid).0 == tid
            };

            if result {
                if self.inner.acquire() {
                    Log::unlock_on_commit(&self.inner.lock as *const _ as u64, journal);
                    true
                } else {
                    #[cfg(not(any(feature = "no_pthread", windows)))] 
                    libc::pthread_mutex_unlock(lock);

                    #[cfg(any(feature = "no_pthread", windows))] 
                    intrinsics::atomic_store_rel(lock, 0);

                    panic!("Cannot have multiple instances of MutexGuard");
                }
            } else {
                false
            }
        }
    }


    /// Attempts to acquire this lock.
    /// 
    /// If the lock could not be acquired at this time, then [`Err`] is returned.
    /// Otherwise, an RAII guard is returned. The lock will be unlocked when the
    /// owner transaction ends.
    ///
    /// This function does not block.
    ///
    /// # Errors
    ///
    /// If another user of this mutex holds a guard, then this call will return
    /// failure if the mutex would otherwise be acquired.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    /// use std::thread;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let obj = P::open::<Parc<PMutex<i32>>>("foo.pool", O_CF).unwrap();
    ///
    /// let a = Parc::demote(&obj);
    /// thread::spawn(move || {
    ///     transaction(|j| {
    ///         if let Some(obj) = a.promote(j) {
    ///             let mut lock = obj.try_lock(j);
    ///             if let Ok(ref mut mutex) = lock {
    ///                 **mutex = 10;
    ///             } else {
    ///                 println!("try_lock failed");
    ///             }
    ///         }
    ///     }).unwrap();
    /// }).join().expect("thread::spawn failed");
    /// 
    /// transaction(|j| {
    ///     assert_eq!(*obj.lock(j), 10);
    /// }).unwrap();
    /// ```
    /// 
    /// [`PMutex`]: ../default/type.PMutex.html
    pub fn try_lock<'a>(&'a self, journal: &'a Journal<A>) -> TryLockResult<MutexGuard<'a, T, A>> {
        if self.raw_trylock(journal) {
            unsafe { Ok(MutexGuard::new(self, journal)) }
        } else {
            Err(TryLockError::WouldBlock)
        }
    }
}

impl<T: RootObj<A>, A: MemPool> RootObj<A> for Mutex<T, A> {
    fn init(journal: &Journal<A>) -> Self {
        Mutex::new(T::init(journal), journal)
    }
}

impl<T: fmt::Debug, A: MemPool> fmt::Debug for Mutex<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.data.fmt(f)
    }
}

pub struct MutexGuard<'a, T: 'a, A: MemPool> {
    lock: &'a Mutex<T, A>,
    journal: *const Journal<A>,
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for MutexGuard<'_, T, A> {}
impl<T: ?Sized, A: MemPool> !Send for MutexGuard<'_, T, A> {}
unsafe impl<T: Sync, A: MemPool> Sync for MutexGuard<'_, T, A> {}

impl<T: fmt::Debug, A: MemPool> fmt::Debug for MutexGuard<'_, T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display, A: MemPool> fmt::Display for MutexGuard<'_, T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<'mutex, T, A: MemPool> MutexGuard<'mutex, T, A> {
    unsafe fn new(
        lock: &'mutex Mutex<T, A>,
        journal: &'mutex Journal<A>,
    ) -> MutexGuard<'mutex, T, A> {
        MutexGuard { lock, journal }
    }
}

impl<T, A: MemPool> Deref for MutexGuard<'_, T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &(*self.lock.data.get()).1 }
    }
}

impl<T: PSafe, A: MemPool> DerefMut for MutexGuard<'_, T, A> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.lock.get_mut(&*self.journal) }
    }
}

impl<T, A: MemPool> Drop for MutexGuard<'_, T, A> {
    fn drop(&mut self) {
        self.lock.inner.release()
    }
}

#[cfg(not(any(feature = "no_pthread", windows)))]
pub unsafe fn init_lock(mutex: *mut libc::pthread_mutex_t, attr: *mut libc::pthread_mutexattr_t) {
    *mutex = libc::PTHREAD_MUTEX_INITIALIZER;
    let result = libc::pthread_mutexattr_init(attr);
    debug_assert_eq!(result, 0);
    let result =
        libc::pthread_mutexattr_settype(attr, libc::PTHREAD_MUTEX_RECURSIVE);
    debug_assert_eq!(result, 0);
    let result = libc::pthread_mutex_init(mutex, attr);
    debug_assert_eq!(result, 0);
    let result = libc::pthread_mutexattr_destroy(attr);
    debug_assert_eq!(result, 0);
}
