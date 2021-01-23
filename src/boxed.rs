//! A persistent pointer type for persistent memory allocation

use crate::alloc::MemPool;
use crate::cell::RootObj;
use crate::clone::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::ll::*;
use crate::{PSafe, VSafe, TxOutSafe};
use std::cmp::Ordering;
use std::convert::From;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::Unpin;
use std::mem;
use std::ops::{Deref,DerefMut};
use std::pin::Pin;
use std::ptr::{self, NonNull};

/// A pointer type for persistent heap allocation.
///
/// If `Pbox` is mutable, the underlying data can mutate after taking a log.
/// It is necessary because compound types containing a `Pbox` may provide
/// interior mutability (via [`LogCell`] or [`LogRefCell`]) though which the
/// `Pbox` become mutably available. The log taken for interior mutability works
/// only on the pointer value and does not include the referent object. Therefore,
/// `Pbox` provides a logging mechanism to provide mutable dereferencing.
/// 
/// # Examples
/// 
/// Create a boxed object in the persistent memory
/// 
/// ```
/// use corundum::default::*;
///
/// type P = BuddyAlloc;
///
/// let _p = P::open_no_root("foo.pool", O_CF).unwrap();
/// 
/// transaction(|j| {
///     let five = Pbox::new(5, j);
///     assert_eq!(*five, 5);
/// }).unwrap();
/// ```
/// 
/// # Examples
///
/// Move a value from the stack to the persistent memory by creating a `Pbox`:
///
/// ```
/// # use corundum::default::*;
/// # type P = BuddyAlloc;
/// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
/// transaction(|j| {
///     let val: u8 = 5;
///     let boxed: Pbox<u8> = Pbox::new(val, j);
/// }).unwrap();
/// ```
///
/// Move a value from a `Pbox` back to the stack by dereferencing:
///
/// ```
/// # use corundum::default::*;
/// # type P = BuddyAlloc;
/// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
/// transaction(|j| {
///     let boxed: Pbox<u8> = Pbox::new(5, j);
///     let val: u8 = *boxed;
/// }).unwrap();
/// ```
///
/// Creating a recursive data structure:
///
/// ```
/// # use corundum::default::*;
/// # type P = BuddyAlloc;
/// #[derive(Debug)]
/// enum List<T: PSafe> {
///     Cons(T, Pbox<List<T>>),
///     Nil,
/// }
///
/// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
/// transaction(|j| {
///     let list: List<i32> = List::Cons(1, Pbox::new(List::Cons(2, Pbox::new(List::Nil, j)), j));
///     println!("{:?}", list);
/// }).unwrap();
/// ```
///
/// This will print `Cons(1, Cons(2, Nil))`.
///
/// Recursive structures must be boxed, because if the definition of `Cons`
/// looked like this:
///
/// ```compile_fail,E0072
/// # enum List<T> {
/// Cons(T, List<T>),
/// # }
/// ```
/// 
/// It wouldn't work. This is because the size of a `List` depends on how many
/// elements are in the list, and so we don't know how much memory to allocate
/// for a `Cons`. By introducing a `Pbox<T>`, which has a defined size, we know
/// how big `Cons` needs to be.
/// 
/// [`LogCell`]: ../cell/struct.LogCell.html
/// [`LogRefCell`]: ../cell/struct.LogRefCell.html
/// [`Logger`]: ../stm/trait.Logger.html
pub struct Pbox<T: PSafe + ?Sized, A: MemPool>(Ptr<T, A>, u8);

impl<T: ?Sized, A: MemPool> !TxOutSafe for Pbox<T, A> {}

impl<A: MemPool, T: ?Sized> !Send for Pbox<T, A> {}
// impl<A: MemPool, T: ?Sized> !Sync for Pbox<T, A> {}
impl<A: MemPool, T: ?Sized> !VSafe for Pbox<T, A> {}

impl<T: PSafe, A: MemPool> Pbox<T, A> {
    /// Allocates memory on the persistent heap and then places `x` into it.
    ///
    /// This doesn't actually allocate if `T` is zero-sized.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # use corundum::boxed::Pbox;
    /// Heap::transaction(|j| {
    ///     let five = Pbox::new(5, j);
    /// }).unwrap();
    /// ```
    pub fn new(x: T, journal: &Journal<A>) -> Pbox<T, A> {
        if mem::size_of::<T>() == 0 {
            Pbox(Ptr::dangling(), 0)
        } else {
            unsafe {
                let p = A::new(x, journal);
                Pbox(Ptr::from_mut(p), 0)
            }
        }
    }

    pub fn off(&self) -> u64 {
        self.0.off()
    }

    /// Constructs a new Pbox with uninitialized contents.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # type P = BuddyAlloc;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// P::transaction(|j| {
    ///     let mut five = Pbox::<u32>::new_uninit(j);
    ///     
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         five.as_mut_ptr().write(5);
    ///     
    ///         five.assume_init()
    ///     };
    ///     
    ///     assert_eq!(*five, 5)
    /// }).unwrap()
    /// ```
    pub fn new_uninit(journal: &Journal<A>) -> Pbox<mem::MaybeUninit<T>, A> {
        let p = unsafe { A::new_uninit::<mem::MaybeUninit<T>>(journal) };
        Pbox(Ptr::from_mut(p), 0)
    }

    /// Constructs a new `Pbox` with uninitialized contents, with the memory
    /// being filled with `0` bytes.
    ///
    /// See [`MaybeUninit::zeroed`][zeroed] for examples of correct and incorrect usage
    /// of this method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::default::*;
    /// # type P = BuddyAlloc;
    /// # let _p = P::open_no_root("foo.pool", O_CF).unwrap();
    /// P::transaction(|j| {
    ///     let zero = Pbox::<u32>::new_zeroed(j);
    ///     let zero = unsafe { zero.assume_init() };
    ///
    ///     assert_eq!(*zero, 0)
    /// }).unwrap()
    /// ```
    ///
    /// [zeroed]: std::mem::MaybeUninit#method.zeroed
    pub fn new_zeroed(journal: &Journal<A>) -> Pbox<mem::MaybeUninit<T>, A> {
        unsafe {
            let mut uninit = Self::new_uninit(journal);
            ptr::write_bytes::<T>(uninit.as_mut().as_mut_ptr(), 0, 1);
            uninit
        }
    }

    /// Constructs a new `Pin<Pbox<T, A>>`. If `T` does not implement `Unpin`, then
    /// `x` will be pinned in memory and unable to be moved.
    #[inline(always)]
    pub fn pin(x: T, journal: &Journal<A>) -> Pin<Pbox<T, A>> {
        Self::new(x, journal).into()
    }
}

impl<T: PSafe, A: MemPool> Pbox<mem::MaybeUninit<T>, A> {
    /// Converts to `Pbox<T, A>`.
    ///
    /// # Safety
    ///
    /// As with [`MaybeUninit::assume_init`],
    /// it is up to the caller to guarantee that the value
    /// really is in an initialized state.
    /// Calling this when the content is not yet fully initialized
    /// causes immediate undefined behavior.
    ///
    /// [`MaybeUninit::assume_init`]: std::mem::MaybeUninit#method.assume_init
    ///
    #[inline]
    pub unsafe fn assume_init(self) -> Pbox<T, A> {
        Pbox::from_raw(Pbox::into_raw(self) as *mut T)
    }
}

impl<T: PSafe, A: MemPool> Pbox<T, A> {
    /// Initializes boxed data with `value` in-place if it is `None`
    ///
    /// This function should not be called from a transaction as it updates
    /// data without taking high-level logs. If transaction is unsuccessful,
    /// there is no way to recover data.
    /// However, it is safe to use it outside a transaction because it uses
    /// low-level logs to provide safety for a single update without drop.
    /// A dynamic check at the beginning makes sure of that.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    /// 
    /// type P = BuddyAlloc;
    ///
    /// let root = P::open::<Option<Pbox<i32>>>("foo.pool", O_CF).unwrap();
    ///
    /// Pbox::initialize(&*root, 25);
    /// 
    /// let value = **root.as_ref().unwrap();
    /// assert_eq!(value, 25);
    /// ```
    ///
    pub fn initialize(boxed: &Option<Pbox<T, A>>, value: T) -> crate::result::Result<()> {
        assert!(
            !Journal::<A>::is_running(),
            "Pbox::initialize() cannot be used inside a transaction"
        );
        match boxed {
            Some(_) => Err("already initialized".to_string()),
            None => if A::valid(boxed) {
                unsafe {
                    let new = A::atomic_new(value);
                    let bnew = Some(Pbox::<T, A>::from_raw(new.0));
                    let src = crate::utils::as_slice64(&bnew);
                    let mut base = A::off_unchecked(boxed);
                    for i in src {
                        A::log64(base, *i, new.3);
                        base += 8;
                    }
                    persist_obj(boxed);
                    A::perform(new.3);
                }
                Ok(())
            } else {
                Err("The box object is not in the PM".to_string())
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Pbox<T, A> {
    /// Constructs a box from a raw pointer.
    ///
    /// After calling this function, the raw pointer is owned by the
    /// resulting `Pbox`. Specifically, the `Pbox` destructor will call
    /// the destructor of `T` and free the allocated memory. For this
    /// to be safe, the memory must have been allocated in accordance
    /// with the [memory layout] used by `Pbox` .
    ///
    /// # Safety
    ///
    /// This function is unsafe because improper use may lead to
    /// memory problems. For example, a double-free may occur if the
    /// function is called twice on the same raw pointer.
    ///
    /// [memory layout]: index.html#memory-layout
    #[inline]
    pub unsafe fn from_raw(raw: *mut T) -> Self {
        Pbox(Ptr::new_unchecked(raw), 0)
    }

    /// Consumes the `Pbox`, returning a wrapped raw pointer.
    ///
    /// The pointer will be properly aligned and non-null.
    ///
    /// After calling this function, the caller is responsible for the
    /// memory previously managed by the `Pbox`. In particular, the
    /// caller should properly destroy `T` and release the memory, taking
    /// into account the [memory layout] used by `Pbox`. The easiest way to
    /// do this is to convert the raw pointer back into a `Pbox` with the
    /// [`Pbox::from_raw`] function, allowing the `Pbox` destructor to perform
    /// the cleanup.
    ///
    /// Note: this is an associated function, which means that you have
    /// to call it as `Pbox::into_raw(b)` instead of `b.into_raw()`. This
    /// is so that there is no conflict with a method on the inner type.
    ///
    #[inline]
    pub fn into_raw(b: Pbox<T, A>) -> *mut T {
        Pbox::into_raw_non_null(b).as_ptr()
    }

    /// Consumes the `Pbox`, returning the wrapped pointer as `NonNull<T>`.
    ///
    /// After calling this function, the caller is responsible for the
    /// memory previously managed by the `Pbox`. In particular, the
    /// caller should properly destroy `T` and release the memory. The
    /// easiest way to do so is to convert the `NonNull<T>` pointer
    /// into a raw pointer and back into a `Pbox` with the [`Pbox::from_raw`]
    /// function.
    ///
    /// Note: this is an associated function, which means that you have
    /// to call it as `Pbox::into_raw_non_null(b)`
    /// instead of `b.into_raw_non_null()`. This
    /// is so that there is no conflict with a method on the inner type.
    ///
    /// [`Pbox::from_raw`]: struct.Pbox.html#method.from_raw
    ///
    #[inline]
    pub fn into_raw_non_null(b: Pbox<T, A>) -> NonNull<T> {
        unsafe { NonNull::new_unchecked(Pbox::into_unique(b).as_mut_ptr()) }
    }

    #[inline]
    #[doc(hidden)]
    pub(crate) fn into_unique(b: Pbox<T, A>) -> Ptr<T, A> {
        let unique = b.0.clone();
        mem::forget(b);
        // Pbox is kind-of a library type, but recognized as a "unique pointer" by
        // Stacked Borrows.  This function here corresponds to "reborrowing to
        // a raw pointer", but there is no actual reborrow here -- so
        // without some care, the pointer we are returning here still carries
        // the tag of `b`, with `Ptr` permission.
        // We round-trip through a mutable reference to avoid that.
        unsafe { Ptr::new_unchecked(unique.get_mut() as *mut T) }
    }

    /// Consumes and leaks the `Pbox`, returning a mutable reference,
    /// `&'a mut T`. Note that the type `T` must outlive the chosen lifetime
    /// `'a`. If the type has only static references, or none at all, then this
    /// may be chosen to be `'static`.
    ///
    /// This function is mainly useful for data that lives for the remainder of
    /// the program's life. Dropping the returned reference will cause a memory
    /// leak. If this is not acceptable, the reference should first be wrapped
    /// with the [`Pbox::from_raw`] function producing a `Pbox`. This `Pbox` can
    /// then be dropped which will properly destroy `T` and release the
    /// allocated memory.
    ///
    /// Note: this is an associated function, which means that you have
    /// to call it as `Pbox::leak(b)` instead of `b.leak()`. This
    /// is so that there is no conflict with a method on the inner type.
    ///
    /// # Safety
    /// 
    /// This function is considered unsafe in persistent memory programming
    /// because memory leak is permanent and undesirable.
    /// 
    /// [`Pbox::from_raw`]: struct.Pbox.html#method.from_raw
    ///
    /// # Examples
    ///
    /// Simple usage:
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # use corundum::boxed::Pbox;
    /// Heap::transaction(|j| unsafe {
    ///     let x = Pbox::new(41, j);
    ///     let static_ref: &'static mut usize = Pbox::leak(x);
    ///     *static_ref += 1;
    ///     assert_eq!(*static_ref, 42);
    /// }).unwrap();
    /// ```
    ///
    #[inline]
    pub unsafe fn leak<'a>(b: Pbox<T, A>) -> &'a mut T
    where
        T: 'a, // Technically not needed, but kept to be explicit.
    {
        &mut *Pbox::into_raw(b)
    }

    /// Converts a `Pbox<T, A>` into a `Pin<Pbox<T, A>>`
    ///
    /// This conversion does not allocate on the heap and happens in place.
    ///
    /// This is also available via [`From`].
    pub fn into_pin(boxed: Pbox<T, A>) -> Pin<Pbox<T, A>>
    where
        T: Sized,
    {
        // It's not possible to move or replace the insides of a `Pin<Pbox<T, A>>`
        // when `T: !Unpin`,  so it's safe to pin it directly without any
        // additional requirements.
        unsafe { Pin::new_unchecked(boxed) }
    }

    pub unsafe fn as_mut(&mut self) -> &mut T {
        self.0.as_mut()
    }

    fn get_ref(&self) -> &T {
        self.0.as_ref()
    }
}

unsafe impl<#[may_dangle] T: PSafe + ?Sized, A: MemPool> Drop for Pbox<T, A> {
    fn drop(&mut self) {
        unsafe {
            let p = self.0.as_mut();
            std::ptr::drop_in_place(p);
            A::free(p);
        }
    }
}

impl<T: Default + PSafe, A: MemPool> RootObj<A> for Pbox<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Pbox<T, A> {
        Pbox::new(T::default(), journal)
    }
}

impl<T: RootObj<A> + PSafe, A: MemPool> RootObj<A> for Pbox<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Pbox<T, A> {
        Pbox::new(T::init(journal), journal)
    }
}

impl<T: PSafe + PClone<A> + ?Sized, A: MemPool> PClone<A> for Pbox<T, A> {
    /// Returns a new box with a `pclone()` of this box's contents.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # use corundum::boxed::Pbox;
    /// use corundum::clone::PClone;
    ///
    /// Heap::transaction(|j| {
    ///     let x = Pbox::new(5, j);
    ///     let y = x.pclone(j);
    ///     
    ///     // The value is the same
    ///     assert_eq!(x, y);
    ///     
    ///     // But they are unique objects
    ///     assert_ne!(&*x as *const i32, &*y as *const i32);
    /// }).unwrap();
    /// ```
    #[inline]
    default fn pclone(&self, journal: &Journal<A>) -> Pbox<T, A> {
        Pbox::new((**self).pclone(journal), journal)
    }
}

impl<T: PSafe + PartialEq + ?Sized, A: MemPool> PartialEq for Pbox<T, A> {
    #[inline]
    fn eq(&self, other: &Pbox<T, A>) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    #[inline]
    fn ne(&self, other: &Pbox<T, A>) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<T: PSafe + PartialOrd + ?Sized, A: MemPool> PartialOrd for Pbox<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Pbox<T, A>) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
    #[inline]
    fn lt(&self, other: &Pbox<T, A>) -> bool {
        PartialOrd::lt(&**self, &**other)
    }
    #[inline]
    fn le(&self, other: &Pbox<T, A>) -> bool {
        PartialOrd::le(&**self, &**other)
    }
    #[inline]
    fn ge(&self, other: &Pbox<T, A>) -> bool {
        PartialOrd::ge(&**self, &**other)
    }
    #[inline]
    fn gt(&self, other: &Pbox<T, A>) -> bool {
        PartialOrd::gt(&**self, &**other)
    }
}

impl<T: PSafe + Ord + ?Sized, A: MemPool> Ord for Pbox<T, A> {
    #[inline]
    fn cmp(&self, other: &Pbox<T, A>) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<T: PSafe + Eq + ?Sized, A: MemPool> Eq for Pbox<T, A> {}

impl<T: PSafe + Hash + ?Sized, A: MemPool> Hash for Pbox<T, A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: PSafe + Hasher + ?Sized, A: MemPool> Hasher for Pbox<T, A> {
    fn finish(&self) -> u64 {
        (**self).finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        unsafe { self.as_mut().write(bytes) }
    }
    fn write_u8(&mut self, i: u8) {
        unsafe { self.as_mut().write_u8(i) }
    }
    fn write_u16(&mut self, i: u16) {
        unsafe { self.as_mut().write_u16(i) }
    }
    fn write_u32(&mut self, i: u32) {
        unsafe { self.as_mut().write_u32(i) }
    }
    fn write_u64(&mut self, i: u64) {
        unsafe { self.as_mut().write_u64(i) }
    }
    fn write_u128(&mut self, i: u128) {
        unsafe { self.as_mut().write_u128(i) }
    }
    fn write_usize(&mut self, i: usize) {
        unsafe { self.as_mut().write_usize(i) }
    }
    fn write_i8(&mut self, i: i8) {
        unsafe { self.as_mut().write_i8(i) }
    }
    fn write_i16(&mut self, i: i16) {
        unsafe { self.as_mut().write_i16(i) }
    }
    fn write_i32(&mut self, i: i32) {
        unsafe { self.as_mut().write_i32(i) }
    }
    fn write_i64(&mut self, i: i64) {
        unsafe { self.as_mut().write_i64(i) }
    }
    fn write_i128(&mut self, i: i128) {
        unsafe { self.as_mut().write_i128(i) }
    }
    fn write_isize(&mut self, i: isize) {
        unsafe { self.as_mut().write_isize(i) }
    }
}

// impl<T: PSafe, A: MemPool> From<T> for Pbox<T, A> {
//     /// Converts a generic type `T` into a `Pbox<T, A>`
//     ///
//     /// The conversion allocates on the heap and moves `t`
//     /// from the stack into it.
//     ///
//     /// # Examples
//     /// ```rust
//     /// # use corundum::boxed::Pbox;
//     /// let x = 5;
//     /// let boxed = Pbox::<_>::new(5);
//     ///
//     /// assert_eq!(Pbox::from(x), boxed);
//     /// ```
//     fn from(t: T, journal: &Journal<A>) -> Self {
//         Pbox::new(t, journal)
//     }
// }

// impl<T: PSafe, A: MemPool> From<&mut T> for Pbox<T, A> {
//     fn from(x: &mut T) -> Self {
//         Pbox(Ptr::new(x).unwrap())
//     }
// }

impl<T: PSafe, A: MemPool> From<Pbox<T, A>> for Pin<Pbox<T, A>> {
    /// Converts a `Pbox<T, A>` into a `Pin<Pbox<T, A>>`
    ///
    /// This conversion does not allocate on the heap and happens in place.
    fn from(boxed: Pbox<T, A>) -> Self {
        Pbox::into_pin(boxed)
    }
}

impl<T: PSafe + fmt::Display + ?Sized, A: MemPool> fmt::Display for Pbox<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: PSafe + fmt::Debug + ?Sized, A: MemPool> fmt::Debug for Pbox<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for Pbox<T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.as_ref()
    }
}

impl<T: PSafe, A: MemPool> DerefMut for Pbox<T, A> {
    fn deref_mut(&mut self) -> &mut T {
        let d = self.0.as_mut();
        if self.1 == 0 && A::valid(&self.1) {
            let journal = Journal::<A>::try_current()
                .expect("Unrecoverable data modification").0;
            unsafe {
                d.take_log(&*journal, Notifier::NonAtomic(Ptr::from_ref(&self.1)));
            }
        }
        d
    }
}

impl<T: PSafe + ?Sized, A: MemPool> AsRef<T> for Pbox<T, A> {
    fn as_ref(&self) -> &T {
        self.get_ref()
    }
}

// impl<T: PSafe + BorrowMut<A> + ?Sized, A: MemPool> BorrowMut<A> for Pbox<T, A> {
//     type Target = T::Target;

//     fn borrow_mut(&self, journal: &Journal<A>) -> RefMut<'_, Self::Target, A> {
//         self.0.as_ref().borrow_mut(journal)
//     }
// }

impl<T: PSafe + ?Sized, A: MemPool> Unpin for Pbox<T, A> {}
