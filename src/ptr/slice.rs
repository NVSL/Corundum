use crate::stm::Journal;
use crate::alloc::MemPool;
use crate::alloc::PmemUsage;
use crate::*;
use std::marker::PhantomData;
use std::ops::Index;

/// A persistent fat pointer with offset and capacity
#[derive(Eq)]
pub struct Slice<T: PSafe, A: MemPool> {
    off: u64,
    cap: usize,
    dummy: [A; 0],
    marker: PhantomData<[T]>,
}

/// `Ptr` pointers are not `Send` because the data they reference may be aliased.
// N.B., this impl is unnecessary, but should provide better error messages.
impl<A: MemPool, T> !Send for Slice<T, A> {}

/// `Ptr` pointers are not `Sync` because the data they reference may be aliased.
// N.B., this impl is unnecessary, but should provide better error messages.
impl<A: MemPool, T> !Sync for Slice<T, A> {}
impl<A: MemPool, T> !TxOutSafe for Slice<T, A> {}

unsafe impl<T: PSafe, A: MemPool> PSafe for Slice<T, A> {}

impl<T: PSafe, A: MemPool> Slice<T, A> {

    /// Creates a new fat pointer given a slice
    pub unsafe fn new(x: &[T]) -> Self {
        if x.len() == 0 {
            Self::from_off_cap(u64::MAX, 0)
        } else {
            Self::from_off_cap(A::off_unchecked(x), x.len())
        }
    }

    /// Sets the capacity to zero
    pub fn empty() -> Self {
        Self::from_off_cap(u64::MAX, 0)
    }

    /// Returns true if the capacity is zero
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cap == 0
    }

    #[inline]
    pub(crate) const fn from_off_cap(off: u64, cap: usize) -> Self {
        Slice {
            off,
            cap,
            dummy: [],
            marker: PhantomData,
        }
    }

    /// Returns a reference to the object at index `i`
    #[inline]
    pub fn get(&self, i: usize) -> &T {
        assert!(i < self.cap, "index out of range");
        unsafe {
            A::deref_mut(self.off + i as u64 * std::mem::size_of::<T>() as u64).unwrap()
        }
    }

    /// Returns a mutable reference to the object at index `i`
    #[inline]
    pub fn get_mut(&self, i: usize) -> &mut T {
        assert!(i < self.cap, "index out of range");
        unsafe {
            A::deref_mut(self.off + i as u64 * std::mem::size_of::<T>() as u64).unwrap()
        }
    }

    /// Returns a mutable reference to the object at index `i` without checking
    /// the boundaries
    #[inline]
    pub unsafe fn get_unchecked(&self, i: usize) -> &mut T {
        A::get_mut_unchecked(self.off + i as u64 * std::mem::size_of::<T>() as u64)
    }

    /// Returns the offset
    #[inline]
    pub fn off(&self) -> u64 {
        self.off
    }

    /// Returns the capacity of the fat pointer
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    /// Returns the mutable reference of the value
    pub(crate) fn as_mut(&mut self) -> &mut T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    #[inline]
    /// Returns the reference of the value
    pub(crate) fn as_ref(&self) -> &T {
        unsafe { A::get_unchecked(self.off) }
    }

    #[inline]
    /// Returns the mutable raw pointer of the value
    pub(crate) fn as_mut_ptr(&self) -> *mut T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    #[inline]
    /// Returns the mutable raw pointer of the value
    pub(crate) fn as_ptr(&self) -> *const T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    /// Converts the fat pointer into a slice of type `&[T]`
    #[inline]
    pub unsafe fn as_slice(&self) -> &[T] {
        Self::to_slice(self.off, self.cap)
    }

    #[inline]
    pub(crate) fn to_slice<'a>(off: u64, len: usize) -> &'a [T] {
        if len == 0 {
            &mut []
        } else {
            unsafe { A::deref_slice_unchecked(off, len) }
        }
    }

    #[inline]
    pub(crate) fn as_slice_mut(&mut self) -> &mut [T] {
        if self.cap == 0 {
            &mut []
        } else {
            unsafe { A::deref_slice_unchecked_mut(self.off, self.cap) }
        }
    }

    /// Divides one slice into two at an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// # Panics
    ///
    /// Panics if `mid > len`.
    /// 
    pub unsafe fn split_at(&mut self, mid: usize) -> (&[T], &[T]) {
        let slice = self.as_slice();
        slice.split_at(mid)
    }

    /// Divides one mutable slice into two at an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// # Panics
    ///
    /// Panics if `mid > len`.
    /// 
    #[inline]
    pub unsafe fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T]) {
        let slice = self.as_slice_mut();
        slice.split_at_mut(mid)
    }

    #[inline]
    pub(crate) fn set_cap(&mut self, new_cap: usize) {
        self.cap = new_cap
    }

    #[inline]
    /// Creates a new copy of data and returns a `Slice` pointer
    ///
    /// # Safety
    ///
    /// The compiler would not drop the copied data. Developer has the
    /// responsibility of deallocating inner value. Also, it does not clone the
    /// inner value. Instead, it just copies the memory.
    /// 
    pub unsafe fn dup(&self, j: &Journal<A>) -> Slice<T, A> {
        if self.is_empty() {
            Self::empty()
        } else {
            Self::from(A::new_copy(self.as_slice(), j))
        }
    }
}

impl<T: PSafe, A: MemPool> Index<usize> for Slice<T, A> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.get(i)
    }
}

impl<A: MemPool, T: PSafe> From<&[T]> for Slice<T, A> {
    #[inline]
    fn from(other: &[T]) -> Self {
        Self::from_off_cap(A::off(other).unwrap(), other.len())
    }
}

impl<A: MemPool, T: PSafe> From<&mut [T]> for Slice<T, A> {
    #[inline]
    fn from(other: &mut [T]) -> Self {
        Self::from_off_cap(A::off(other).unwrap(), other.len())
    }
}

impl<A: MemPool + Copy, T: PSafe + Copy> Copy for Slice<T, A> {}

impl<A: MemPool, T: PSafe> Clone for Slice<T, A> {
    fn clone(&self) -> Self {
        Self::from_off_cap(self.off, self.cap)
    }
}

impl<A: MemPool, T: PSafe> PmemUsage for Slice<T, A> {
    fn size_of() -> usize {
        std::mem::size_of::<T>() + std::mem::size_of::<Self>()
    }
}

impl<A: MemPool, T: PSafe> Default for Slice<T, A> {
    fn default() -> Self {
        Slice::empty()
    }
}

impl<A: MemPool, T: PSafe> PartialEq for Slice<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.off == other.off && self.cap == other.cap
    }
}
