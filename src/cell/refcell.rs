use crate::ptr::{LogNonNull,NonNull};
use crate::convert::PFrom;
use crate::alloc::MemPool;
use crate::cell::VCell;
use crate::stm::Journal;
use crate::*;
use std::cell::UnsafeCell;
use std::fmt::{self, Debug, Display};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};

#[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
use crate::cell::TCell;

/// A persistent memory location with safe interior mutability and dynamic
/// borrow checking
///
/// This is one of the safe ways to provide interior mutability for pointer
/// wrappers. It takes a log, if it was not already taken, before exposing the
/// mutable reference to the protected data.
///
/// To borrow the value immutably, [`borrow()`](#method.borrow) can be used. Its
/// return value is a [`Ref<T>`](./struct.RefMut.html). The function
/// [`borrow_mut()`](#method.borrow_mut) returns the inner value wrapped in
/// [`RefMut<T>`](./struct.RefMut.html). The borrowing rules is checked
/// dynamically when the user tries to borrow the value. It panics if any of
/// the following situation happens:
/// 
/// * Borrowing the value mutably while it was already borrowed immutably
/// * Borrowing the value mutably twice
/// * Borrowing the value immutably while it was already borrowed mutably
///
/// It does not implement [`Sync`], so it is not possible to share `PRefCell`
/// between threads. To provide thread-safe interior mutability, use
/// [`PMutex`].
/// 
/// [`PRefCell`] is an alias name in the pool module for `PRefCell`.
///
/// [`Sync`]: std::marker::Sync
/// [`PMutex`]: ../sync/mutex/struct.PMutex.html
/// [`RwLock`]: ../sync/mutex/struct.RwLock.html
/// [`PRefCell`]: ../alloc/default/type.PRefCell.html
///
pub struct PRefCell<T: PSafe + ?Sized, A: MemPool> {
    heap: PhantomData<A>,

    borrow: VCell<i8, A>,

    #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
    temp: TCell<Option<*mut T>, A>,

    #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
    value: UnsafeCell<T>,

    #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))]
    value: UnsafeCell<(u8, T)>,
}

impl<T: PSafe + ?Sized, A: MemPool> RefUnwindSafe for PRefCell<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> UnwindSafe for PRefCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> TxInSafe for PRefCell<T, A> {}
impl<T: ?Sized, A: MemPool> !TxOutSafe for PRefCell<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> PSafe for PRefCell<T, A> {}

/// Safe to transfer between thread boundaries
unsafe impl<T: PSafe + ?Sized, A: MemPool> Send for PRefCell<T, A> {}

/// Not safe for thread data sharing
impl<T: ?Sized, A: MemPool> !Sync for PRefCell<T, A> {}

impl<T: ?Sized, A: MemPool> !PSend for PRefCell<T, A> {}

impl<T: PSafe, A: MemPool> PRefCell<T, A> {
    /// Creates a new instance of `PRefCell` with the given value
    pub fn new(value: T) -> Self {
        Self::def(value)
    }

    #[inline]
    fn def(value: T) -> Self {
        PRefCell {
            heap: PhantomData,
            borrow: VCell::new(0),

            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
            temp: TCell::invalid(None),

            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
            value: UnsafeCell::new(value),

            #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))]
            value: UnsafeCell::new((0, value)),
        }
    }

    /// Replaces the wrapped value with a new one, returning the old value,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`std::mem::replace`](../mem/fn.replace.html).
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(PRefCell::new(5), j);
    ///     
    ///     let old_value = cell.replace(6, j);
    ///     assert_eq!(old_value, 5);
    ///     assert_eq!(*cell.borrow(), 6);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn replace(&self, t: T, j: &Journal<A>) -> T {
        std::mem::replace(&mut *self.borrow_mut(j), t)
    }

    /// Replaces the wrapped value with a new one computed from `f`, returning
    /// the old value, without deinitializing either one.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(PRefCell::new(5), j);
    ///     
    ///     let old_value = cell.replace_with(j, |&mut old| old + 1);
    ///     assert_eq!(old_value, 5);
    ///     assert_eq!(*cell.borrow(), 6);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn replace_with<F: FnOnce(&mut T) -> T>(&self, j: &Journal<A>, f: F) -> T {
        let mut_borrow = &mut *self.borrow_mut(j);
        let replacement = f(mut_borrow);
        std::mem::replace(mut_borrow, replacement)
    }

    /// Swaps the wrapped value of `self` with the wrapped value of `other`,
    /// without deinitializing either one.
    ///
    /// This function corresponds to [`std::mem::swap`](../mem/fn.swap.html).
    ///
    /// # Panics
    ///
    /// Panics if the value in either `RefCell` is currently borrowed.
    ///
    /// # Examples
    /// 
    /// ```
    /// use corundum::default::*;
    ///
    /// let _pool = BuddyAlloc::open_no_root("foo.pool", O_CF);
    ///     
    /// BuddyAlloc::transaction(|j| {
    ///     let c1 = Pbox::new(PRefCell::new(5i32), j);
    ///     let c2 = Pbox::new(PRefCell::new(10i32), j);
    ///     c1.swap(&c2, j);
    ///     assert_eq!(10, c1.take(j));
    ///     assert_eq!(5, c2.take(j));
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn swap(&self, other: &Self, j: &Journal<A>) {
        std::mem::swap(&mut *self.borrow_mut(j), &mut *other.borrow_mut(j))
    }
}

// impl<T: PSafe + Default, A: MemPool> Default for PRefCell<T, A> {
//     fn default() -> Self {
//         Self::def(T::default())
//     }
// }

impl<T: PSafe + RootObj<A>, A: MemPool> RootObj<A> for PRefCell<T, A> {
    default fn init(j: &Journal<A>) -> Self {
        Self::def(T::init(j))
    }
}

impl<T: PSafe + Display + ?Sized, A: MemPool> Display for PRefCell<T, A> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.as_ref().fmt(fmt)
    }
}

impl<T: PSafe + Debug + ?Sized, A: MemPool> Debug for PRefCell<T, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.as_ref().fmt(f)
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PRefCell<T, A> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn self_mut(&self) -> &mut Self {
        unsafe {
            let ptr: *const Self = self;
            &mut *(ptr as *mut Self)
        }
    }

    #[inline]
    #[allow(clippy::mut_from_ref)]
    /// Takes a log and returns a mutable reference to the underlying data.
    ///
    /// This call borrows the `UnsafeCell` mutably (at compile-time) which
    /// guarantees that we possess the only reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    ///
    /// let _pool = BuddyAlloc::open_no_root("foo.pool", O_CF);
    ///     
    /// BuddyAlloc::transaction(|j| {
    ///     let c1 = Pbox::new(PRefCell::new(5i32), j);
    ///     let c2 = Pbox::new(PRefCell::new(10i32), j);
    ///     c1.swap(&c2, j);
    ///     assert_eq!(10, *c1.borrow());
    ///     assert_eq!(5, *c2.borrow());
    /// }).unwrap();
    /// ```
    pub fn get_mut(&mut self, journal: &Journal<A>) -> &mut T {
        let inner = unsafe { &mut *self.value.get() };
        self.take_log(journal);

        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] unsafe {
            if let Some(tmp) = *self.temp {
                &mut *tmp
            } else {
                &mut *inner
            }
        }
    
        #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
            &mut inner.1
        }
    }

    #[inline]
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
    /// use corundum::cell::PRefCell;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let root = P::open::<PRefCell<i32,P>>("foo.pool", O_CF).unwrap();
    /// 
    /// unsafe {
    ///     let mut data = root.as_mut();
    ///     *data = 20;
    /// }
    /// 
    /// ```
    pub unsafe fn as_mut(&self) -> &mut T {
        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
            if let Some(tmp) = *self.temp {
                &mut *tmp
            } else {
                &mut *self.value.get()
            }
        }
    
        #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
            &mut (*self.value.get()).1
        }
    }

    #[inline]
    #[allow(clippy::mut_from_ref)]
    #[inline]
    /// Returns an immutable reference of the inner value
    pub(crate) fn as_ref(&self) -> &T {
        unsafe {
            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
                if let Some(tmp) = *self.temp {
                    &*tmp
                } else {
                    &*self.value.get()
                }
            }
        
            #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
                &(*self.value.get()).1
            }
        }
    }

    #[inline]
    /// Immutably borrows from an owned value.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(PRefCell::new(5), j);
    ///     
    ///     assert_eq!(*cell.borrow(), 5);
    /// }).unwrap();
    /// ```
    #[track_caller]
    pub fn borrow(&self) -> Ref<'_, T, A> {
        unsafe {
            let borrow = &self.borrow as *const VCell<i8, A> as *mut VCell<i8, A>;
            assert!(**borrow <= 0, "Value was already mutably borrowed");
            **borrow = -1;
        }
        Ref { value: self, phantom: PhantomData }
    }

    #[inline]
    /// Returns a clone of the underlying data
    /// 
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let cell = Pbox::new(PRefCell::new(5), j);
    ///     
    ///     assert_eq!(cell.read(), 5);
    /// }).unwrap();
    /// ```
    pub fn read(&self) -> T
    where
        T: std::clone::Clone,
    {
        self.as_ref().clone()
    }

    #[inline]
    #[track_caller]
    pub(crate) fn take_log(&self, journal: &Journal<A>) {
        unsafe {
            let inner = &mut *self.value.get();
            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
                if self.temp.is_none() {
                    self.temp.as_mut().replace(journal.draft(&inner));
                }
            }
            #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
                use crate::ptr::Ptr;
                use crate::stm::{Notifier, Logger};
                if inner.0 == 0 {
                    assert!(A::valid(inner), "The object is not in the pool's valid range");
                    inner.1.take_log(journal, Notifier::NonAtomic(Ptr::from_ref(&inner.0)));
                }
            }
        }
    }
}

impl<T: PSafe + PClone<A>, A: MemPool> PFrom<Ref<'_, T, A>, A> for PRefCell<T, A> {
    /// Crates a new `PRefCell` and drops the `Ref`
    /// 
    /// After calling this function, the `Ref` won't be available anymore. It 
    /// will be possible to borrow the `PRefCell` mutably. The new
    /// `PRefCell` has a new location with the same data.
    fn pfrom(other: Ref<'_, T, A>, j: &Journal<A>) -> Self {
        Self::def(other.pclone(j))
    }
}

impl<T: PSafe + PClone<A>, A: MemPool> PFrom<RefMut<'_, T, A>, A> for PRefCell<T, A> {
    /// Crates a new `PRefCell` and drops the `Ref`
    /// 
    /// After calling this function, the `Ref` won't be available anymore. It 
    /// will be possible to borrow the `PRefCell` mutably. The new
    /// `PRefCell` has a new location with the same data.
    fn pfrom(other: RefMut<'_, T, A>, j: &Journal<A>) -> Self {
        Self::def(other.pclone(j))
    }
}

impl<T: PSafe, A: MemPool> PFrom<T, A> for PRefCell<T, A> {
    /// Crates a new `PRefCell`
    fn pfrom(value: T, _j: &Journal<A>) -> Self {
        Self::new(value)
    }
}


impl<T: PSafe, A: MemPool> From<T> for PRefCell<T, A> {
    /// Crates a new `PRefCell`
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: PSafe + Default, A: MemPool> PRefCell<T, A> {
    /// Takes the value of the cell, leaving `Default::default()` in its place.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    ///
    /// Heap::transaction(|j| {
    ///     let c = Pbox::new(PRefCell::new(5), j);
    ///     let five = c.take(j);
    ///
    ///     assert_eq!(five, 5);
    ///     assert_eq!(*c.borrow(), 0);
    /// }).unwrap();
    /// ```
    pub fn take(&self, journal: &Journal<A>) -> T {
        self.replace(Default::default(), journal)
    }
}

impl<T: PSafe, A: MemPool> PRefCell<T, A> {
    /// Mutably borrows from an owned value.
    ///
    /// It returns a `RefMut` type for interior mutability which takes a log of
    /// data when dereferenced mutably. This method requires accessing current
    /// journal which is provided in [`transaction`](../stm/fn.transaction.html).
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::boxed::Pbox;
    ///
    /// let cell=Heap::transaction(|j| {
    ///     let cell = Pbox::new(PRefCell::new(5), j);
    ///     {
    ///         let mut cell = cell.borrow_mut(j);
    ///         *cell = 10;
    ///     }
    ///     assert_eq!(*cell.borrow(), 10);
    /// }).unwrap();
    /// ```
    ///
    #[inline]
    #[track_caller]
    pub fn borrow_mut(&self, journal: &Journal<A>) -> RefMut<'_, T, A> {
        let borrow = self.borrow.as_mut();
        assert!(*borrow >= 0, "Value was already immutably borrowed");
        assert!(*borrow == 0, "Value was already mutably borrowed");
        *borrow = 1;
        RefMut {
            value: unsafe { &mut *(self as *const Self as *mut Self) },
            journal,
            phantom: PhantomData
        }
    }

    /// Returns a `LogNonNull` pointer to the data
    /// 
    /// # Safety
    /// 
    /// `LogNonNull` does not dynamically check the borrowing rules. Also, it
    /// may outlive the data, leading to a segmentation fault. It is not
    /// recommended to use this function without necessary manual checks.
    /// 
    pub unsafe fn as_non_null_mut(&self, journal: &Journal<A>) -> LogNonNull<T, A> {
        let inner = &mut *self.value.get();
        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
            LogNonNull::new_unchecked(inner, journal)
        }
        #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
            LogNonNull::new_unchecked(&mut inner.1, &mut inner.0, journal)
        }
    }

    /// Returns a `NonNull` pointer to the data
    pub fn as_non_null(&self) -> NonNull<T> {
        unsafe { 
            let inner = &mut *self.value.get();
            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
                NonNull::new_unchecked(inner)
            }
            #[cfg(not(any(feature = "use_pspd", feature = "use_vspd")))] {
                NonNull::new_unchecked(&mut inner.1)
            }
        }
    }
}

use crate::clone::PClone;
impl<T: PSafe + PClone<A>, A: MemPool> PClone<A> for PRefCell<T, A> {
    #[inline]
    fn pclone(&self, j: &Journal<A>) -> PRefCell<T, A> {
        PRefCell::new(self.as_ref().pclone(j))
    }
}

impl<T: PSafe + Clone, A: MemPool> Clone for PRefCell<T, A> {
    #[inline]
    fn clone(&self) -> PRefCell<T, A> {
        PRefCell::new(self.as_ref().clone())
    }
}

impl<T: PSafe + PartialEq + ?Sized, A: MemPool> PartialEq for PRefCell<T, A> {
    #[inline]
    fn eq(&self, other: &PRefCell<T, A>) -> bool {
        *self.as_ref() == *other.as_ref()
    }
}

impl<T: PSafe + Eq + ?Sized, A: MemPool> Eq for PRefCell<T, A> {}

impl<T: PSafe + PartialOrd + ?Sized, A: MemPool> PartialOrd for PRefCell<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &PRefCell<T, A>) -> Option<std::cmp::Ordering> {
        self.as_ref().partial_cmp(&*other.as_ref())
    }

    #[inline]
    fn lt(&self, other: &PRefCell<T, A>) -> bool {
        *self.as_ref() < *other.as_ref()
    }

    #[inline]
    fn le(&self, other: &PRefCell<T, A>) -> bool {
        *self.as_ref() <= *other.as_ref()
    }

    #[inline]
    fn gt(&self, other: &PRefCell<T, A>) -> bool {
        *self.as_ref() > *other.as_ref()
    }

    #[inline]
    fn ge(&self, other: &PRefCell<T, A>) -> bool {
        *self.as_ref() >= *other.as_ref()
    }
}

impl<T: PSafe + Ord + ?Sized, A: MemPool> Ord for PRefCell<T, A> {
    #[inline]
    fn cmp(&self, other: &PRefCell<T, A>) -> std::cmp::Ordering {
        self.as_ref().cmp(&*other.as_ref())
    }
}

pub struct Ref<'b, T: 'b + PSafe + ?Sized, A: MemPool> {
    value: *const PRefCell<T, A>,
    phantom: PhantomData<&'b T>
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Ref<'_, T, A> {}
impl<T: ?Sized, A: MemPool> !Send for Ref<'_, T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for Ref<'_, T, A> {}

impl<'b, T: PSafe + ?Sized, A: MemPool> Ref<'b, T, A> {
    /// Copies a `Ref`.
    ///
    /// The `PRefCell` is already immutably borrowed, so this cannot fail. To
    /// be able to borrow mutably, all `Ref`s should go out of scope.
    ///
    /// This is an associated function that needs to be used as
    /// `Ref::clone(...)`. A `Clone` implementation or a method would interfere
    /// with the widespread use of `r.borrow().clone()` to clone the contents of
    /// a `PRefCell`.
    #[inline]
    #[track_caller]
    pub fn clone(orig: &Ref<'b, T, A>) -> Ref<'b, T, A> {
        let borrow = unsafe {(*orig.value).borrow.as_mut()};
        assert!(*borrow > i8::MIN);
        *borrow -= 1;
        Ref { value: orig.value, phantom: PhantomData }
    }

    /// Convert into a reference to the underlying data.
    ///
    /// The underlying `RefCell` can never be mutably borrowed from again and will always appear
    /// already immutably borrowed. It is not a good idea to leak more than a constant number of
    /// references. The `RefCell` can be immutably borrowed again if only a smaller number of leaks
    /// have occurred in total.
    ///
    /// This is an associated function that needs to be used as
    /// `Ref::leak(...)`. A method would interfere with methods of the
    /// same name on the contents of a `RefCell` used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(cell_leak)]
    /// use std::cell::{RefCell, Ref};
    /// let cell = RefCell::new(0);
    ///
    /// let value = Ref::leak(cell.borrow());
    /// assert_eq!(*value, 0);
    ///
    /// assert!(cell.try_borrow().is_ok());
    /// assert!(cell.try_borrow_mut().is_err());
    /// ```
    pub fn leak(orig: Ref<'b, T, A>) -> &'b T {
        // By forgetting this Ref we ensure that the borrow counter in the RefCell can't go back to
        // UNUSED within the lifetime `'b`. Resetting the reference tracking state would require a
        // unique reference to the borrowed RefCell. No further mutable references can be created
        // from the original cell.
        unsafe {(*orig.value).as_ref()}
    }
}

#[cfg(feature = "refcell_lifetime_change")]
impl<T: PSafe + ?Sized, A: MemPool> Ref<'_, T, A> {
    /// Creates a new owner of `Ref` for a broader lifetime useful for
    /// letting it out from a function
    /// 
    /// This associative function obtains the ownership of the original `Ref`
    /// so that the number of immutably borrowers doesn't change
    /// 
    pub fn own<'a, 'b>(orig: Ref<'a, T, A>) -> Ref<'b, T, A> {
        let res = Ref {
            value: orig.value,
            phantom: PhantomData
        };
        std::mem::forget(orig);
        res
    }

    /// Returns the `&PRefCell` and drops the `Ref`
    pub fn into_inner<'a>(orig: Ref<'a, T, A>) -> &'a PRefCell<T, A> {
        let inner = orig.value;
        std::mem::drop(orig);
        unsafe { &*inner }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for Ref<'_, T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe {(*self.value).as_ref()}
    }
}

impl<T: fmt::Display + PSafe, A: MemPool> fmt::Display for Ref<'_, T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe {(*self.value).fmt(f)}
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Drop for Ref<'_, T, A> {
    fn drop(&mut self) {
        let borrow = unsafe {(*self.value).borrow.as_mut()};
        *borrow += 1;
    }
}

pub struct RefMut<'b, T: 'b + PSafe + ?Sized, A: MemPool> {
    value: *mut PRefCell<T, A>,
    journal: *const Journal<A>,
    phantom: PhantomData<&'b T>
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for RefMut<'_, T, A> {}
impl<T: ?Sized, A: MemPool> !Send for RefMut<'_, T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for RefMut<'_, T, A> {}

#[cfg(feature = "refcell_lifetime_change")]
impl<T: PSafe + ?Sized, A: MemPool> RefMut<'_, T, A> {
    /// Creates a new owner of `RefMut` for a broader lifetime useful for
    /// letting it out from a function
    /// 
    /// This associative function obtains the ownership of the original `RefMut`
    /// so that there will be still only one mutable owner to the underlying
    /// data.
    /// 
    pub fn own<'a, 'b>(orig: RefMut<'a, T, A>) -> RefMut<'b, T, A> {
        let res = RefMut {
            value: orig.value,
            journal: orig.journal,
            phantom: PhantomData
        };
        std::mem::forget(orig);
        res
    }

    /// Returns the `&PRefCell` and drops the `RefMut`
    pub fn into_inner<'a>(orig: RefMut<'a, T, A>) -> &'a PRefCell<T, A> {
        let inner = orig.value;
        std::mem::drop(orig);
        unsafe { &*inner }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> RefMut<'_, T, A> {
    /// Converts `RefMut` into a mutable reference within the same lifetime
    pub fn into_mut<'a>(r: RefMut<'a, T, A>) -> &'a mut T {
        unsafe { (*r.value).as_mut() }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for RefMut<'_, T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { (*self.value).as_ref() }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> DerefMut for RefMut<'_, T, A> {
    #[inline]
    #[track_caller]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { (*self.value).get_mut(&*self.journal) }
    }
}

impl<T: fmt::Display + PSafe + ?Sized, A: MemPool> fmt::Display for RefMut<'_, T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.value).fmt(f) }
    }
}

impl<T: fmt::Debug + PSafe + ?Sized, A: MemPool> fmt::Debug for RefMut<'_, T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.value).fmt(f) }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Drop for RefMut<'_, T, A> {
    fn drop(&mut self) {
        unsafe {
            let borrow = (*self.value).borrow.as_mut();
            *borrow -= 1;
        }
    }
}
