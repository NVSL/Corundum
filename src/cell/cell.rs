use crate::clone::PClone;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};
// use std::ops::{DerefMut,Deref};
use crate::alloc::MemPool;
use crate::ptr::Ptr;
use crate::stm::{Journal, Notifier, Logger};
use crate::{PSafe,TxInSafe,TxOutSafe};
use std::{fmt, mem, ptr};

/// A persistent mutable memory location with recoverability
///
/// This is one of the safe ways to provide interior mutability for pointer
/// wrappers. It takes a log, if it was not already taken, before updating the
/// value.
///
/// Using [`get()`](#method.get) function, you can obtain a copy of data. To 
/// update data, you can use [`set()`](#method.set) which writes a log to the
/// given journal before mutation.
///
/// It does not implement [`Sync`], so it is not possible to share `LogCell`
/// between threads. To provide thread-safe interior mutability, use
/// [`Mutex`].
/// 
/// [`PCell`] is a compact version of `LogCell` tha can be find in the pool
/// module.
///
/// [`Sync`]: std::marker::Sync
/// [`Mutex`]: ../sync/mutex/struct.Mutex.html
/// [`PCell`]: ../alloc/default/type.PCell.html
/// 
pub struct LogCell<T: PSafe + ?Sized, A: MemPool> {
    heap: PhantomData<A>,
    value: UnsafeCell<(u8, T)>,
}

unsafe impl<T: PSafe + Send + ?Sized, A: MemPool> Send for LogCell<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> RefUnwindSafe for LogCell<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> UnwindSafe for LogCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> TxInSafe for LogCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> PSafe for LogCell<T, A> {}

impl<T: ?Sized, A: MemPool> !TxOutSafe for LogCell<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for LogCell<T, A> {}

impl<T: PSafe + Default, A: MemPool> Default for LogCell<T, A> {
    fn default() -> Self {
        LogCell {
            heap: PhantomData,
            value: UnsafeCell::new((0, T::default())),
        }
    }
}

impl<T: PSafe + PartialEq + Copy, A: MemPool> PartialEq for LogCell<T, A> {
    #[inline]
    fn eq(&self, other: &LogCell<T, A>) -> bool {
        self.get() == other.get()
    }
}

impl<T: PSafe + Eq + Copy, A: MemPool> Eq for LogCell<T, A> {}

impl<T: PSafe + PartialOrd + Copy, A: MemPool> PartialOrd for LogCell<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &LogCell<T, A>) -> Option<Ordering> {
        self.get().partial_cmp(&other.get())
    }

    #[inline]
    fn lt(&self, other: &LogCell<T, A>) -> bool {
        self.get() < other.get()
    }

    #[inline]
    fn le(&self, other: &LogCell<T, A>) -> bool {
        self.get() <= other.get()
    }

    #[inline]
    fn gt(&self, other: &LogCell<T, A>) -> bool {
        self.get() > other.get()
    }

    #[inline]
    fn ge(&self, other: &LogCell<T, A>) -> bool {
        self.get() >= other.get()
    }
}

impl<T: PSafe + Ord + Copy, A: MemPool> Ord for LogCell<T, A> {
    #[inline]
    fn cmp(&self, other: &LogCell<T, A>) -> Ordering {
        self.get().cmp(&other.get())
    }
}

impl<T: PSafe, A: MemPool> LogCell<T, A> {
    /// Creates a new `LogCell` containing the given value.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = LogCell::new(5, j);
    /// }).unwrap();
    /// ```
    #[inline]
    pub const fn new(value: T, _j: &Journal<A>) -> LogCell<T, A> {
        LogCell {
            heap: PhantomData,
            value: UnsafeCell::new((0, value)),
        }
    }

    /// Sets the contained value.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(LogCell::new(5, j), j);
    ///     c.set(10, j);
    /// }).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// If `LogCell` is not in the persistent memory, it will raise an 'invalid
    /// address' error. To make sure that the `LogCell` is in the persistent
    /// memory, use dynamic allocation using [`Pbox`] as shown above.
    ///
    /// [`Pbox`]: ../../boxed/struct.Pbox.html
    #[inline]
    pub fn set(&self, val: T, journal: &Journal<A>) {
        let old = self.replace(val, journal);
        drop(old);
    }

    /// Swaps the values of two Cells.
    /// 
    /// Difference with `std::mem::swap` is that this function doesn't require
    /// `&mut` reference. It takes a log of both sides, if required, and then
    /// swaps the values.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::default::*;
    /// use crndm::cell::LogCell;
    ///
    /// let _pool = BuddyAlloc::open_no_root("foo.pool", O_CF).unwrap();
    ///     
    /// BuddyAlloc::transaction(|j| {
    ///     let c1 = Pbox::new(LogCell::new(5i32, j), j);
    ///     let c2 = Pbox::new(LogCell::new(10i32, j), j);
    ///     c1.swap(&c2, j);
    ///     assert_eq!(10, c1.get());
    ///     assert_eq!(5, c2.get());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn swap(&self, other: &Self, journal: &Journal<A>) {
        let this = unsafe { &mut (*self.value.get()).1 };
        let that = unsafe { &mut (*other.value.get()).1 };
        if ptr::eq(this, that) {
            return;
        }
        self.take_log(journal);
        other.take_log(journal);

        // SAFETY: This can be risky if called from separate threads, but `LogCell`
        // is `!Sync` so this won't happen. This also won't invalidate any
        // pointers since `LogCell` makes sure nothing else will be pointing into
        // either of these `LogCell`s.
        unsafe {
            ptr::swap(this, that);
        }
    }

    /// Replaces the contained value, and returns it.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(LogCell::new(5, j), j);
    ///     assert_eq!(cell.get(), 5);
    ///     assert_eq!(cell.replace(10, j), 5);
    ///     assert_eq!(cell.get(), 10);
    /// }).unwrap();
    /// ```
    pub fn replace(&self, val: T, journal: &Journal<A>) -> T {
        self.take_log(journal);
        // SAFETY: This can cause data races if called from a separate thread,
        // but `LogCell` is `!Sync` so this won't happen.
        mem::replace(unsafe { &mut (*self.value.get()).1 }, val)
    }

    /// Unwraps the value.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = LogCell::new(5, j);
    ///     let five = c.into_inner();
    ///
    ///     assert_eq!(five, 5);
    /// }).unwrap();
    /// ```
    pub fn into_inner(self) -> T {
        self.value.into_inner().1
    }

    #[inline]
    fn self_mut(&self) -> &mut Self {
        unsafe { &mut *(self as *const Self as *mut Self) }
    }

    /// Increments the contained value by `val`.
    #[inline]
    pub fn add(&self, val: T, journal: &Journal<A>)
    where
        T: std::ops::AddAssign,
    {
        let v = self.self_mut().get_mut(journal);
        *v += val;
    }

    /// Subtracts the contained value by `val`.
    #[inline]
    pub fn sub(&self, val: T, journal: &Journal<A>)
    where
        T: std::ops::SubAssign,
    {
        let v = self.self_mut().get_mut(journal);
        *v -= val;
    }

    /// Multiplies the contained value with `val`.
    #[inline]
    pub fn mul(&self, val: T, journal: &Journal<A>)
    where
        T: std::ops::MulAssign,
    {
        let v = self.self_mut().get_mut(journal);
        *v *= val;
    }

    /// Divides the contained value with `val`.
    #[inline]
    pub fn div(&self, val: T, journal: &Journal<A>)
    where
        T: std::ops::DivAssign,
    {
        let v = self.self_mut().get_mut(journal);
        *v /= val;
    }

    /// Divides the contained value with `val` and keeps the reminding.
    #[inline]
    pub fn rem(&self, val: T, journal: &Journal<A>)
    where
        T: std::ops::RemAssign,
    {
        let v = self.self_mut().get_mut(journal);
        *v %= val;
    }
}

impl<T: PSafe, A: MemPool> LogCell<T, A> {
    /// Returns a copy of the contained value.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = LogCell::new(5, j);
    ///     let five = c.get();
    ///     
    ///     assert_eq!(five, 5);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn get(&self) -> T where T: Copy {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `LogCell` is `!Sync` so this won't happen.
        unsafe { (*self.value.get()).1 }
    }

    #[inline]
    pub fn get_ref(&self) -> &T {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `LogCell` is `!Sync` so this won't happen.
        unsafe { &(*self.value.get()).1 }
    }

    /// Updates the contained value using a function and returns the new value.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(cell_update)]
    ///
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(LogCell::new(5, j), j);
    ///     let new = c.update(j, |x| x + 1);
    ///
    ///     assert_eq!(new, 6);
    ///     assert_eq!(c.get(), 6);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn update<F>(&self, journal: &Journal<A>, f: F) -> T
    where
        F: FnOnce(T) -> T,
        T: Copy
    {
        let old = self.get();
        let new = f(old);
        self.set(new, journal);
        new
    }

    /// Updates the contained value using an updater function with an immutable
    /// reference to the inner value
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(LogCell::new(LogCell::new(5, j), j), j);
    ///     c.update_inplace(|x| x.set(6, j));
    ///
    ///     assert_eq!(c.get_ref().get(), 6);
    /// }).unwrap();
    /// ```
    pub fn update_inplace<F>(&self, f: F) 
    where
        F: FnOnce(&T)
    {
        f(unsafe { &(*self.value.get()).1 })
    }

    /// Updates the contained value using an updater function with a mutable
    /// reference to the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(cell_update)]
    ///
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(LogCell::new(5, j), j);
    ///     c.update_inplace_mut(j, |x| { *x = 6 });
    ///
    ///     assert_eq!(c.get(), 6);
    /// }).unwrap();
    /// ```
    pub fn update_inplace_mut<F>(&self, journal: &Journal<A>, f: F) 
    where
        F: FnOnce(&mut T)
    {
        self.take_log(journal);
        f(unsafe { &mut (*self.value.get()).1 })
    }
}

impl<T: PSafe + ?Sized, A: MemPool> LogCell<T, A> {
    #[inline]
    pub(crate) fn take_log(&self, journal: &Journal<A>) {
        unsafe {
            let inner = &mut *self.value.get();
            if inner.0 == 0 {
                inner.1.take_log(journal, Notifier::NonAtomic(Ptr::from_ref(&inner.0)));
            }
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows `LogCell` mutably (at compile-time) which guarantees
    /// that we possess the only reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let mut c = Pbox::new(LogCell::new(5, j), j);
    ///     let mut n = c.get_mut(j);
    ///     *n += 1;
    ///
    ///     assert_eq!(c.get(), 6);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn get_mut(&mut self, journal: &Journal<A>) -> &mut T {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `LogCell` is `!Sync` so this won't happen, and `&mut` guarantees
        // unique access.
        self.take_log(journal);
        unsafe { &mut (*self.value.get()).1 }
    }
    
    /// Returns a mutable reference to the underlying data without taking a log
    ///
    /// # Safety
    /// 
    /// This function violates borrow rules as it allows multiple mutable
    /// references.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::default::*;
    /// use crndm::cell::LogCell;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let root = P::open::<LogCell<i32,P>>("foo.pool", O_CF).unwrap();
    /// 
    /// unsafe {
    ///     let mut data = root.as_mut();
    ///     *data = 20;
    /// }
    /// 
    /// ```
    #[inline]
    pub unsafe fn as_mut(&self) -> &mut T {
        &mut (*self.value.get()).1
    }
}

impl<T: PSafe + Default, A: MemPool> LogCell<T, A> {
    /// Takes the value of the cell, leaving `Default::default()` in its place.
    ///
    /// # Examples
    ///
    /// ```
    /// use crndm::alloc::*;
    /// use crndm::boxed::Pbox;
    /// use crndm::cell::LogCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(LogCell::new(5, j), j);
    ///     let five = c.take(j);
    ///
    ///     assert_eq!(five, 5);
    ///     assert_eq!(c.get(), 0);
    /// }).unwrap();
    /// ```
    pub fn take(&self, journal: &Journal<A>) -> T {
        self.replace(Default::default(), journal)
    }
}

impl<T: fmt::Debug + PSafe, A: MemPool> fmt::Debug for LogCell<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.value.get()).1.fmt(f) }
    }
}

impl<T: PSafe + Logger<A> + Copy, A: MemPool> PClone<A> for LogCell<T, A> {
    #[inline]
    fn pclone(&self, j: &Journal<A>) -> LogCell<T, A> {
        unsafe { LogCell::new((*self.value.get()).1, j) }
    }
}