use crate::alloc::MemPool;
use crate::{PSafe, VSafe};
use std::cmp::*;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A persistent memory location containing a volatile data
/// 
/// The underlying data is valid throughout of the course of a single pool
/// lifetime. When the pool is reopened, the data is back to its default value.
/// Type `T` in `VCell<T>` should implement [`Default`] and [`VSafe`].
/// 
/// # Examples
/// 
/// ```
/// use corundum::default::*;
/// use std::cell::RefCell;
/// 
/// type P = BuddyAlloc;
/// 
/// #[derive(Default)]
/// struct Root {
///     v: VCell<RefCell<i32>>
/// }
///
/// let root = P::open::<Root>("foo.pool", O_CF).unwrap();
///     
/// P::transaction(|j| {
///     let mut v = root.v.borrow_mut();
///     assert_eq!(*v, i32::default());
///     *v = 20; // This value is volatile and resets on restart
/// }).unwrap();
/// ```
/// 
/// [`Default`]: std::default::Default
/// [`VSafe`]: ../trait.VSafe.html
pub struct VCell<T: Default + VSafe + ?Sized, A: MemPool> {
    gen: u32,
    phantom: PhantomData<(A, T)>,
    value: *mut T,
}

/// Safe to transfer between thread boundaries
unsafe impl<T: Default + VSafe + ?Sized, A: MemPool> Send for VCell<T, A> {}
unsafe impl<T: Default + VSafe + ?Sized, A: MemPool> PSafe for VCell<T, A> {}

/// Not safe for thread data sharing
impl<T, A: MemPool> !Sync for VCell<T, A> {}

impl<T: Default + VSafe, A: MemPool> VCell<T, A> {
    pub fn new(v: T) -> Self {
        Self {
            gen: A::gen(),
            value: Box::into_raw(Box::new(v)),
            phantom: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn as_mut(&self) -> &mut T {
        unsafe { &mut *(self.deref() as *const T as *mut T) }
    }
}

impl<T: Default + VSafe, A: MemPool> Default for VCell<T, A> {
    fn default() -> Self {
        Self {
            gen: A::gen(),
            value: Box::into_raw(Box::new(T::default())),
            phantom: PhantomData,
        }
    }
}

impl<T: Default + VSafe, A: MemPool> Deref for VCell<T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        use std::intrinsics::*;
        unsafe {
            let self_mut = crate::as_mut(self);
            let gen = &mut (*self_mut).gen;
            while atomic_load_acq(gen) == u32::MAX {}

            let curr = A::gen();
            let old = atomic_load_acq(&self.gen);
            if old != curr {
                if atomic_cxchg_acqrel(gen, old, u32::MAX).0 != curr {
                    std::mem::forget(std::ptr::replace(&mut (*self_mut).value, 
                    Box::into_raw(Box::new(T::default()))));
                    atomic_store_rel(gen, curr);
                }
                while atomic_load_acq(gen) == u32::MAX {}
            }
            &*self.value
        }
    }
}

impl<T: Default + VSafe, A: MemPool> DerefMut for VCell<T, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }
}

impl<T: Default + VSafe + PartialEq + Copy, A: MemPool> PartialEq for VCell<T, A> {
    #[inline]
    fn eq(&self, other: &VCell<T, A>) -> bool {
        self.value == other.value
    }
}

impl<T: Default + VSafe + Eq + Copy, A: MemPool> Eq for VCell<T, A> {}

impl<T: Default + VSafe + PartialOrd + Copy, A: MemPool> PartialOrd for VCell<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &VCell<T, A>) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }

    #[inline]
    fn lt(&self, other: &VCell<T, A>) -> bool {
        self.value < other.value
    }

    #[inline]
    fn le(&self, other: &VCell<T, A>) -> bool {
        self.value <= other.value
    }

    #[inline]
    fn gt(&self, other: &VCell<T, A>) -> bool {
        self.value > other.value
    }

    #[inline]
    fn ge(&self, other: &VCell<T, A>) -> bool {
        self.value >= other.value
    }
}

impl<T: Default + VSafe + Ord + Copy, A: MemPool> Ord for VCell<T, A> {
    #[inline]
    fn cmp(&self, other: &VCell<T, A>) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T: Default + VSafe + PartialEq + Copy, A: MemPool> PartialEq<T> for VCell<T, A> {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        unsafe { *self.value == *other }
    }
}

impl<T: Default + VSafe + PartialOrd + Copy, A: MemPool> PartialOrd<T> for VCell<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &T) -> Option<Ordering> {
        unsafe { (*self.value).partial_cmp(&other) }
    }

    #[inline]
    fn lt(&self, other: &T) -> bool {
        unsafe { *self.value < *other }
    }

    #[inline]
    fn le(&self, other: &T) -> bool {
        unsafe { *self.value <= *other }
    }

    #[inline]
    fn gt(&self, other: &T) -> bool {
        unsafe { *self.value > *other }
    }

    #[inline]
    fn ge(&self, other: &T) -> bool {
        unsafe { *self.value >= *other }
    }
}

impl<T: Default + VSafe + ?Sized, A: MemPool> Drop for VCell<T, A> { 
    fn drop(&mut self) {
        use std::alloc::Layout;
        std::mem::drop(self.value);
        if self.gen == A::gen() {
            unsafe {
                std::alloc::dealloc(self.value as *mut _ as *mut u8, Layout::new::<T>());
            }
        }
    }
}