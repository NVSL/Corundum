use std::marker::PhantomData;
use crate::alloc::MemPool;
use crate::ptr::Ptr;
use crate::stm::{Journal, Notifier, Logger};
use crate::{PSafe, TxOutSafe};
use std::fmt;
use std::ops::{Deref, DerefMut};


/// An unsafe pointer with dereferencing capability
/// 
/// This type is `!PSafe` and its constructor functions are `unsafe`. This is
/// because of `NonNull` treating like a raw pointer. `NonNull` objects are
/// useful for obtaining performance.
/// 
/// [`PNonNull`] is an alias name in the pool module for `LogNonNull`.
/// 
/// [`Prc`]: ../prc/struct.Prc.html
/// [`PNonNull`]: ../alloc/default/type.PNonNull.html
/// 
pub struct NonNull<T: PSafe + ?Sized> {
    ptr: *const T
}

impl<T: PSafe + ?Sized> Copy for NonNull<T> {}
impl<T: PSafe + ?Sized> Clone for NonNull<T> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
        }
    }
}

impl<T: ?Sized> !TxOutSafe for NonNull<T> {}
impl<T: ?Sized> !Send for NonNull<T> {}
impl<T: ?Sized> !Sync for NonNull<T> {}

impl<T: PSafe> NonNull<T> {
    /// Creates a new `NonNull`.
    ///
    /// # Safety
    ///
    /// `ptr` and `logged` must be non-null.
    #[inline]
    pub const fn new_unchecked(ptr: *const T) -> Self {
        // SAFETY: the caller must guarantee that `ptr` is non-null.
        Self {
            ptr,
        } 
    }

    /// Creates a `Some(NonNull)` if `ptr` is not null; otherwise `None`.
    #[inline]
    pub fn new(ptr: *const T) -> Option<Self> {
        if !ptr.is_null() {
            Some(Self::new_unchecked(ptr))
        } else {
            None
        }
    }

    pub fn as_ref<'a, 'b>(&'a self) -> &'b T {
        unsafe { &*self.ptr }
    }
}

impl<T: PSafe + ?Sized> Deref for NonNull<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T { unsafe { &*self.ptr } }
}

impl<T: fmt::Display + PSafe + ?Sized> fmt::Display for NonNull<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.ptr).fmt(f) }
    }
}

impl<T: fmt::Debug + PSafe + ?Sized> fmt::Debug for NonNull<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.ptr).fmt(f) }
    }
}

/// An unsafe pointer with dereferencing and logging capability
/// 
/// This type is `!PSafe` and its constructor functions are `unsafe`. This is
/// because of `LogNonNull` treating like a raw pointer. `LogNonNull` objects are
/// useful for obtaining performance. [`PRefCell`]`::`[`as_non_null_mut()`] 
/// is an alternative to [`PRefCell`]`::`[`borrow_mut()`] which provides unsafe
/// mutable access to the underlying data.
/// 
/// [`PNonNullMut`] is an alias name in the pool module for `LogNonNullMut`.
/// 
/// [`PNonNullMut`]: ../alloc/default/type.PNonNullMut.html
/// 
/// # Safety
/// 
/// * As in raw pointers, `LogNonNull` is `Copy` and is not bounded to a specific
/// lifetime.
/// * It does not follow the borrow mechanism and thus multiple mutable access is
/// possible.
/// 
/// # Examples
/// 
/// ```
/// use corundum::default::*;
/// use corundum::ptr::LogNonNull;
/// 
/// type P = BuddyAlloc;
/// 
/// fn multiply(mut obj: LogNonNull<i32,P>, m: i32) {
///     // Takes a log if required and then provides mut ref to the object
///     *obj *= m;
/// }
/// 
/// let root = P::open::<PRefCell<i32>>("foo.pool", O_CF).unwrap();
/// 
/// P::transaction(|j| {
///     let mut borrow = root.borrow_mut(j);
///     *borrow = 5;
/// 
///     multiply( unsafe { root.as_non_null_mut(j) }, 10 );
/// }).unwrap();
/// 
/// assert_eq!(*root.borrow(), 50);
/// ```
///
/// [`PRefCell`]: ../cell/struct.PRefCell.html
/// [`as_non_null_mut()`]: ../cell/struct.PRefCell.html#method.as_non_null_mut
/// [`borrow_mut()`]: ../cell/struct.PRefCell.html#method.borrow_mut
/// [`PNonNullMut`]: ../alloc/default/type.PNonNullMut.html
/// 
pub struct LogNonNull<T: PSafe + ?Sized, A: MemPool> {
    ptr: *mut T,
    journal: *const Journal<A>,
    logged: *mut u8,
    phantom: PhantomData<*mut T>
}

impl<T: PSafe + ?Sized, A: MemPool> Copy for LogNonNull<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> Clone for LogNonNull<T, A> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            journal: self.journal,
            logged: self.logged,
            phantom: PhantomData
        }
    }
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for LogNonNull<T, A> {}
impl<T: ?Sized, A: MemPool> !Send for LogNonNull<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for LogNonNull<T, A> {}

impl<T: PSafe, A: MemPool> LogNonNull<T, A> {
    /// Creates a new `LogNonNull`.
    ///
    /// # Safety
    ///
    /// `ptr` and `logged` must be non-null.
    #[inline]
    pub const unsafe fn new_unchecked(ptr: *mut T, logged: *mut u8, j: &Journal<A>) -> Self {
        // SAFETY: the caller must guarantee that `ptr` is non-null.
        Self {
            ptr,
            journal: j as *const _,
            logged,
            phantom: PhantomData
        } 
    }

    /// Creates a `Some(LogNonNull)` if `ptr` is not null; otherwise `None`.
    #[inline]
    pub unsafe fn new(ptr: *mut T, logged: *mut u8, j: &Journal<A>) -> Option<Self> {
        if !ptr.is_null() && !logged.is_null() {
            Some(Self::new_unchecked(ptr, logged, j))
        } else {
            None
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for LogNonNull<T, A> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T { unsafe { &*self.ptr } }
}

impl<T: PSafe + ?Sized, A: MemPool> DerefMut for LogNonNull<T, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            let value = &mut *self.ptr;
            if *self.logged == 0 {
                value.take_log(&*self.journal, Notifier::NonAtomic(Ptr::from_raw(self.logged)));
            }
            value
        }
    }
}

impl<T: fmt::Display + PSafe + ?Sized, A: MemPool> fmt::Display for LogNonNull<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.ptr).fmt(f) }
    }
}

impl<T: fmt::Debug + PSafe + ?Sized, A: MemPool> fmt::Debug for LogNonNull<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (*self.ptr).fmt(f) }
    }
}