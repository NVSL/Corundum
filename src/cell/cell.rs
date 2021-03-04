use crate::clone::PClone;
use crate::alloc::MemPool;
use crate::stm::{Journal, Logger};
use crate::*;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::{fmt, mem, ptr};

#[cfg(feature = "use_scratchpad")]
use crate::cell::TCell;

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
/// It does not implement [`Sync`], so it is not possible to share `PCell`
/// between threads. To provide thread-safe interior mutability, use
/// [`PMutex`].
/// 
/// [`PCell`] is a compact version of `PCell` tha can be find in the pool
/// module.
///
/// [`Sync`]: std::marker::Sync
/// [`PMutex`]: ../sync/mutex/struct.PMutex.html
/// [`PCell`]: ../alloc/default/type.PCell.html
/// 
pub struct PCell<T: PSafe + ?Sized, A: MemPool> {
    heap: PhantomData<A>,

    #[cfg(feature = "use_scratchpad")]
    temp: TCell<Option<*mut T>, A>,

    #[cfg(feature = "use_scratchpad")]
    value: UnsafeCell<T>,

    #[cfg(not(feature = "use_scratchpad"))]
    value: UnsafeCell<(u8, T)>,
}

unsafe impl<T: PSafe + Send + ?Sized, A: MemPool> Send for PCell<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> RefUnwindSafe for PCell<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> UnwindSafe for PCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> TxInSafe for PCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> PSafe for PCell<T, A> {}

impl<T: ?Sized, A: MemPool> !TxOutSafe for PCell<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for PCell<T, A> {}
impl<T: ?Sized, A: MemPool> !PSend for PCell<T, A> {}

impl<T: PSafe + Default, A: MemPool> Default for PCell<T, A> {
    fn default() -> Self {
        PCell {
            heap: PhantomData,
    
            #[cfg(feature = "use_scratchpad")]
            temp: TCell::invalid(None),

            #[cfg(feature = "use_scratchpad")]
            value: UnsafeCell::new(T::default()),

            #[cfg(not(feature = "use_scratchpad"))]
            value: UnsafeCell::new((0, T::default())),
        }
    }
}

impl<T: PSafe + PartialEq + Copy, A: MemPool> PartialEq for PCell<T, A> {
    #[inline]
    fn eq(&self, other: &PCell<T, A>) -> bool {
        self.get() == other.get()
    }
}

impl<T: PSafe + Eq + Copy, A: MemPool> Eq for PCell<T, A> {}

impl<T: PSafe + PartialOrd + Copy, A: MemPool> PartialOrd for PCell<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &PCell<T, A>) -> Option<Ordering> {
        self.get().partial_cmp(&other.get())
    }

    #[inline]
    fn lt(&self, other: &PCell<T, A>) -> bool {
        self.get() < other.get()
    }

    #[inline]
    fn le(&self, other: &PCell<T, A>) -> bool {
        self.get() <= other.get()
    }

    #[inline]
    fn gt(&self, other: &PCell<T, A>) -> bool {
        self.get() > other.get()
    }

    #[inline]
    fn ge(&self, other: &PCell<T, A>) -> bool {
        self.get() >= other.get()
    }
}

impl<T: PSafe + Ord + Copy, A: MemPool> Ord for PCell<T, A> {
    #[inline]
    fn cmp(&self, other: &PCell<T, A>) -> Ordering {
        self.get().cmp(&other.get())
    }
}

impl<T: PSafe, A: MemPool> PCell<T, A> {
    /// Creates a new `PCell` containing the given value.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let c = PCell::new(5);
    /// }).unwrap();
    /// ```
    #[inline]
    pub const fn new(value: T) -> PCell<T, A> {
        PCell {
            heap: PhantomData,

            #[cfg(feature = "use_scratchpad")]
            temp: TCell::invalid(None),

            #[cfg(feature = "use_scratchpad")]
            value: UnsafeCell::new(value),

            #[cfg(not(feature = "use_scratchpad"))]
            value: UnsafeCell::new((0, value)),
        }
    }

    /// Sets the contained value.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    /// use corundum::cell::PCell;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PCell::new(5), j);
    ///     c.set(10, j);
    /// }).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// If `PCell` is not in the persistent memory, it will raise an 'invalid
    /// address' error. To make sure that the `PCell` is in the persistent
    /// memory, use dynamic allocation using [`Pbox`] as shown above.
    ///
    /// [`Pbox`]: ../../boxed/struct.Pbox.html
    #[inline]
    #[track_caller]
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
    /// use corundum::default::*;
    ///
    /// let _pool = BuddyAlloc::open_no_root("foo.pool", O_CF).unwrap();
    ///     
    /// BuddyAlloc::transaction(|j| {
    ///     let c1 = Pbox::new(PCell::new(5i32), j);
    ///     let c2 = Pbox::new(PCell::new(10i32), j);
    ///     c1.swap(&c2, j);
    ///     assert_eq!(10, c1.get());
    ///     assert_eq!(5, c2.get());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn swap(&self, other: &Self, journal: &Journal<A>) {
        let this = unsafe { self.as_mut() };
        let that = unsafe { other.as_mut() };
        if ptr::eq(this, that) {
            return;
        }
        self.take_log(journal);
        other.take_log(journal);

        // SAFETY: This can be risky if called from separate threads, but `PCell`
        // is `!Sync` so this won't happen. This also won't invalidate any
        // pointers since `PCell` makes sure nothing else will be pointing into
        // either of these `PCell`s.
        unsafe {
            ptr::swap(this, that);
        }
    }

    #[inline]
    #[track_caller]
    /// Replaces the contained value, and returns it.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    /// use corundum::cell::PCell;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(PCell::new(5), j);
    ///     assert_eq!(cell.get(), 5);
    ///     assert_eq!(cell.replace(10, j), 5);
    ///     assert_eq!(cell.get(), 10);
    /// }).unwrap();
    /// ```
    pub fn replace(&self, val: T, journal: &Journal<A>) -> T {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `PCell` is `!Sync` so this won't happen.

        self.take_log(journal);

        #[cfg(feature = "use_scratchpad")] {
            if let Some(tmp) = *self.temp {
                mem::replace(unsafe { &mut *tmp }, val)
            } else {
                mem::replace(unsafe { &mut *self.value.get() }, val)
            }
        }

        #[cfg(not(feature = "use_scratchpad"))]
        mem::replace(unsafe { &mut (*self.value.get()).1 }, val)
    }

    /// Unwraps the value.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let c = PCell::new(5);
    ///     let five = c.into_inner();
    ///
    ///     assert_eq!(five, 5);
    /// }).unwrap();
    /// ```
    pub fn into_inner(self) -> T {

        #[cfg(feature = "use_scratchpad")] {
            self.value.into_inner()
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            self.value.into_inner().1
        }
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

impl<T: PSafe, A: MemPool> PCell<T, A> {
    /// Returns a copy of the contained value.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let c = PCell::new(5);
    ///     let five = c.get();
    ///     
    ///     assert_eq!(five, 5);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn get(&self) -> T where T: Copy {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `PCell` is `!Sync` so this won't happen.

        unsafe {
            #[cfg(feature = "use_scratchpad")] {
                if let Some(tmp) = *self.temp {
                    *tmp
                } else {
                    *self.value.get()
                }
            }
            
            #[cfg(not(feature = "use_scratchpad"))] {
                (*self.value.get()).1
            }
        }
    }

    #[inline(always)]
    pub fn get_ref(&self) -> &T {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `PCell` is `!Sync` so this won't happen.

        #[cfg(feature = "use_scratchpad")] unsafe {
            if let Some(tmp) = *self.temp {
                &*tmp
            } else {
                &*self.value.get()
            }
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            unsafe { &(*self.value.get()).1 }
        }
    }

    /// Updates the contained value using a function and returns the new value.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(cell_update)]
    ///
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PCell::new(5), j);
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
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PCell::new(PCell::new(5)), j);
    ///     c.update_inplace(|x| x.set(6, j));
    ///
    ///     assert_eq!(c.get_ref().get(), 6);
    /// }).unwrap();
    /// ```
    pub fn update_inplace<F>(&self, f: F) 
    where
        F: FnOnce(&T)
    {
        f(self.get_ref())
    }

    /// Updates the contained value using an updater function with a mutable
    /// reference to the inner value.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(cell_update)]
    ///
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PCell::new(5), j);
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
        f(unsafe { self.as_mut() })
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PCell<T, A> {
    #[inline]
    #[track_caller]
    pub(crate) fn take_log(&self, journal: &Journal<A>) {
        unsafe {
            let inner = &mut *self.value.get();
            #[cfg(feature = "use_scratchpad")] {
                if self.temp.is_none() {
                    self.temp.as_mut().replace(journal.draft(&inner));
                }
            }
            #[cfg(not(feature = "use_scratchpad"))] {
                use crate::ptr::Ptr;
                use crate::stm::Notifier;
                if inner.0 == 0 {
                    assert!(A::valid(inner), "The object is not in the pool's valid range");
                    inner.1.take_log(journal, Notifier::NonAtomic(Ptr::from_ref(&inner.0)));
                }
            }
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows `PCell` mutably (at compile-time) which guarantees
    /// that we possess the only reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    /// use corundum::cell::PCell;
    ///
    /// Heap::transaction(|j| {
    ///     let mut c = Pbox::new(PCell::new(5), j);
    ///     let mut n = c.get_mut(j);
    ///     *n += 1;
    ///
    ///     assert_eq!(c.get(), 6);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn get_mut(&mut self, journal: &Journal<A>) -> &mut T {
        // SAFETY: This can cause data races if called from a separate thread,
        // but `PCell` is `!Sync` so this won't happen, and `&mut` guarantees
        // unique access.

        self.take_log(journal);

        #[cfg(feature = "use_scratchpad")] unsafe {
            if let Some(tmp) = *self.temp {
                &mut *tmp
            } else {
                &mut *self.value.get()
            }
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            unsafe { &mut (*self.value.get()).1 }
        }
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
    /// use corundum::default::*;
    /// use corundum::cell::PCell;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let root = P::open::<PCell<i32,P>>("foo.pool", O_CF).unwrap();
    /// 
    /// unsafe {
    ///     let mut data = root.as_mut();
    ///     *data = 20;
    /// }
    /// 
    /// ```
    #[inline]
    pub unsafe fn as_mut(&self) -> &mut T {
        #[cfg(feature = "use_scratchpad")] {
            if let Some(tmp) = *self.temp {
                &mut *tmp
            } else {
                &mut *self.value.get()
            }
        }
        #[cfg(not(feature = "use_scratchpad"))] {
            &mut (*self.value.get()).1
        }
    }
}

impl<T: PSafe + Default, A: MemPool> PCell<T, A> {
    /// Takes the value of the cell, leaving `Default::default()` in its place.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PCell::new(5), j);
    ///     let five = c.take(j);
    ///
    ///     assert_eq!(five, 5);
    ///     assert_eq!(c.get(), 0);
    /// }).unwrap();
    /// ```
    #[track_caller]
    pub fn take(&self, journal: &Journal<A>) -> T {
        self.replace(Default::default(), journal)
    }
}

impl<T: fmt::Debug + PSafe, A: MemPool> fmt::Debug for PCell<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "use_scratchpad")] {
            unsafe { (*self.value.get()).fmt(f) }
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            unsafe { (*self.value.get()).1.fmt(f) }
        }
    }
}

impl<T: PSafe + Logger<A> + Copy, A: MemPool> PClone<A> for PCell<T, A> {
    #[inline]
    fn pclone(&self, _j: &Journal<A>) -> PCell<T, A> {
        #[cfg(feature = "use_scratchpad")] {
            unsafe { PCell::new(*self.value.get()) }
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            unsafe { PCell::new((*self.value.get()).1) }
        }
    }
}

impl<T: PSafe + Logger<A> + Clone, A: MemPool> Clone for PCell<T, A> {
    #[inline]
    fn clone(&self) -> PCell<T, A> {
        #[cfg(feature = "use_scratchpad")] {
            unsafe { PCell::new((*self.value.get()).clone()) }
        }

        #[cfg(not(feature = "use_scratchpad"))] {
            unsafe { PCell::new((*self.value.get()).1.clone()) }
        }
    }
}