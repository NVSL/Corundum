//! A contiguous growable array type with heap-allocated contents, written Vec<T>

use crate::convert::PFrom;
use crate::alloc::get_idx;
use crate::alloc::MemPool;
use crate::clone::PClone;
use crate::ptr::*;
use crate::stm::*;
use crate::*;
use std::alloc::Layout;
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::ops::Index;
use std::slice::SliceIndex;
use std::vec::Vec as StdVec;
use std::{mem, ptr, slice};

/// A contiguous growable persistent array type, written `Vec<T>` but pronounced
/// 'vector'.
/// 
/// [`PVec`] is an alias name in the pool module for `Vec`.
/// 
/// [`PVec`]: ../alloc/default/type.PVec.html
///
/// # Examples
///
/// ```
/// # use corundum::vec::Vec;
/// # use corundum::alloc::*;
/// Heap::transaction(|j| {
///     let mut vec = Vec::new(j);
///     vec.push(1, j);
///     vec.push(2, j);
///
///     assert_eq!(vec.len(), 2);
///     assert_eq!(vec[0], 1);
///
///     assert_eq!(vec.pop(), Some(2));
///     assert_eq!(vec.len(), 1);
///
///     vec.extend_from_slice(&[1, 2, 3], j);
///
///     for x in vec.as_slice() {
///         println!("{}", x);
///     }
///     assert_eq!(vec, [1, 1, 2, 3]);
///
/// }).unwrap();
/// ```
pub struct Vec<T: PSafe, A: MemPool> {
    buf: FatPtr<T, A>,
    len: usize,
    marker: PhantomData<[T]>,
}

unsafe impl<T: PSafe, A: MemPool> PSafe for Vec<T, A> {}
impl<T, A: MemPool> !Send for Vec<T, A> {}
impl<T, A: MemPool> !Sync for Vec<T, A> {}
impl<T, A: MemPool> !VSafe for Vec<T, A> {}

impl<T: PSafe, A: MemPool> Vec<T, A> {
    /// Creates an empty vector with zero capacity for the pool of the give `Journal`
    pub const fn new(_j: &Journal<A>) -> Self {
        Self::empty()
    }

    /// Creates an empty vector and places `x` into it
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2], j);
    ///
    ///     assert_eq!(vec.len(), 2);
    ///     assert_eq!(vec[0], 1);
    ///
    ///     assert_eq!(vec.pop(), Some(2));
    ///     assert_eq!(vec.len(), 1);
    ///
    ///     vec.extend_from_slice(&[1, 2, 3], j);
    ///
    ///     for x in &*vec {
    ///         println!("{}", x);
    ///     }
    ///
    ///     assert_eq!(vec, [1, 1, 2, 3]);
    /// }).unwrap();
    /// ```
    pub fn from_slice(x: &[T], journal: &Journal<A>) -> Self {
        if x.len() == 0 {
            Self::empty()
        } else {
            let buf = unsafe { A::new_slice(x, journal) };
            Self::from_off_len(unsafe { A::off_unchecked(buf) }, buf.len(), buf.len())
        }
    }

    pub(crate) unsafe fn from_slice_nolog(x: &[T]) -> (Self, usize) {
        if x.len() == 0 {
            (Self::empty(), 0)
        } else {
            let (buf, off, _, z) = A::atomic_new_slice(x);
            (Self::from_off_len(off, buf.len(), buf.len()), z)
        }
    }

    /// Creates an empty `Vec` with the specified capacity
    ///
    /// The vector will be able to hold exactly `capacity` elements without
    /// reallocating. If `capacity` is 0, the vector will not allocate.
    ///
    /// It is important to note that although the returned vector has the
    /// *capacity* specified, the vector will have a zero *length*. For an
    /// explanation of the difference between length and capacity, see
    /// *[Capacity and reallocation]*.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::with_capacity(10, j);
    ///
    ///     // The vector contains no items, even though it has capacity for more
    ///     assert_eq!(vec.len(), 0);
    ///
    ///     // These are all done without reallocating...
    ///     for i in 0..10 {
    ///         vec.push(i, j);
    ///     }
    ///
    ///     // ...but this may make the vector reallocate
    ///     vec.push(11, j);
    /// }).unwrap();
    /// ```
    pub fn with_capacity(cap: usize, j: &Journal<A>) -> Self {
        if cap == 0 {
            Self::empty()
        } else {
            let layout = Layout::array::<T>(cap).unwrap();
            unsafe {
                let buf = A::new_uninit_for_layout(layout.size(), j);
                Self::from_off_len(A::off_unchecked(buf), cap, 0)
            }
        }
    }

    /// Creates an empty vector with zero capacity
    pub const fn empty() -> Self {
        Self::from_off_len(u64::MAX, 0, 0)
    }

    /// Returns `true` if the vector contains no elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut v = Vec::new(j);
    ///     assert!(v.is_empty());
    ///
    ///     v.push(1, j);
    ///     assert!(!v.is_empty());
    /// }).unwrap();
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Splits the collection into two at the given index.
    ///
    /// Returns a newly allocated vector containing the elements in the range
    /// `[at, len)`. After the call, the original vector will be left containing
    /// the elements `[0, at)` with its previous capacity unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `at > len`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1,2,3], j);
    ///     let vec2 = vec.split_off(1, j);
    ///     assert_eq!(vec, [1]);
    ///     assert_eq!(vec2, [2, 3]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn split_off(&mut self, at: usize, j: &Journal<A>) -> Self {
        assert!(at <= self.len(), "`at` out of bounds");

        let other_len = self.len - at;
        let mut other = Vec::with_capacity(other_len, j);

        // Unsafely `set_len` and copy items to `other`.
        unsafe {
            self.set_len(at);
            other.set_len_volatile(other_len);

            ptr::copy_nonoverlapping(
                self.as_ptr().add(at),
                other.as_slice_mut().as_mut_ptr(),
                other.len(),
            );
        }
        other
    }

    /// Creates a `Vec` without allocating memory by specifying the `offset`,
    /// `capacity`, and the `length` of the vector.
    pub(crate) const fn from_off_len(offset: u64, capacity: usize, length: usize) -> Self {
        Self {
            buf: FatPtr::from_off_cap(offset, capacity),
            len: length,
            marker: PhantomData,
        }
    }

    /// Returns raw parts in form of (buf: *mut u8, length: usize, capacity: usize)
    pub(crate) unsafe fn into_raw_parts(&self) -> (*mut T, usize, usize) {
        (
            A::get_mut_unchecked(self.buf.off()),
            self.len,
            self.buf.capacity(),
        )
    }

    #[inline]
    fn get(&self, i: usize) -> &T {
        self.buf.get(i)
    }

    #[inline]
    fn get_mut(&self, i: usize) -> &mut T {
        self.buf.get_mut(i)
    }

    #[inline]
    /// Returns the offset of the vector in the persistent pool
    pub fn off(&self) -> u64 {
        self.buf.off()
    }

    #[inline]
    /// Returns the available capacity of the vector in the persistent pool
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    #[inline]
    /// Forces the length of the vector to `new_len`.
    ///
    /// This is a low-level operation that maintains none of the normal
    /// invariants of the type. Normally changing the length of a vector
    /// is done using one of the safe operations instead, such as
    /// [`truncate`], [`resize`], [`push`], or [`clear`].
    ///
    /// [`truncate`]: #method.truncate
    /// [`resize`]: #method.resize
    /// [`push`]: #method.push
    /// [`clear`]: #method.clear
    ///
    /// # Safety
    ///
    /// - `new_len` must be less than or equal to [`capacity()`].
    /// - The elements at `old_len..new_len` must be initialized.
    ///
    /// [`capacity()`]: #method.capacity
    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.buf.capacity());

        self.len = new_len;
    }

    fn set_len_volatile(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.buf.capacity());
        self.len = new_len;
    }

    #[inline]
    /// Returns the length of the vector
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    /// Consumes the vector and converts it into a slice
    pub fn as_slice(&self) -> &[T] {
        Self::to_slice(self.off(), self.len)
    }

    #[inline]
    pub(crate) fn as_slice_mut(&mut self) -> &mut [T] {
        Self::to_slice_mut(self.off(), self.len)
    }

    #[inline]
    /// Consumes the `corundum::vec::Vec` and converts it into a standard [`std::vec::Vec`](std::vec::Vec)
    pub(crate) unsafe fn as_vec(&mut self) -> StdVec<T> {
        StdVec::from_raw_parts(self.buf.as_mut_ptr(), self.len, self.buf.capacity())
    }

    #[inline]
    fn to_slice<'a>(off: u64, len: usize) -> &'a [T] {
        if len == 0 {
            &mut []
        } else {
            unsafe { A::deref_slice_unchecked(off, len) }
        }
    }

    #[inline]
    fn to_slice_mut<'a>(off: u64, len: usize) -> &'a mut [T] {
        if len == 0 || off == u64::MAX {
            &mut []
        } else {
            unsafe { A::deref_slice_unchecked_mut(off, len) }
        }
    }

    /// Copy all the elements of `other` into `Self`
    ///
    /// # Panics
    ///
    /// Panics if the number of elements in the vector overflows a `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     let mut other = vec![4, 5, 6];
    ///     vec.extend_from_slice(&other, j);
    ///     assert_eq!(vec.as_slice(), [1, 2, 3, 4, 5, 6]);
    /// }).unwrap();
    /// ```
    pub fn extend_from_slice(&mut self, other: &[T], j: &Journal<A>) {
        if other.len() != 0 {
            unsafe {
                let len = self.len;
                let new_cap = self.capacity().max(len + other.len());
                self.reserve(new_cap - self.capacity(), j);
                let ptr = self.buf.as_mut_ptr();
                ptr::copy(other.as_ptr(), ptr.add(len), other.len());
                self.len += other.len();
            }
        }
    }

    #[inline]
    pub fn shrink_to(&mut self, new_cap: usize, j: &Journal<A>) {
        let cap = self.capacity();
        eprintln!("shrink_to");

        // Prevent shrinking to smaller than data
        let new_cap = new_cap.max(self.len);
        if get_idx(new_cap * mem::size_of::<T>()) != get_idx(cap * mem::size_of::<T>()) {
            unsafe {
                let buf = self.as_slice_mut();
                let (rem, left) = buf.split_at_mut(buf.len().min(new_cap));
                if !left.is_empty() {
                    ptr::drop_in_place(left);
                    // FIXME: use power of 2 sizes padding to be able to free memory
                    // of each individual item
                }
                if rem.is_empty() {
                    self.buf = FatPtr::empty();
                } else {
                    let new = A::new_slice(rem, j);
                    self.buf = FatPtr::from_off_cap(A::off_unchecked(new), new.len());
                }
            }
        }
        self.buf.set_cap(new_cap);
    }

    /// Shrinks the capacity of the vector as much as possible.
    ///
    /// It will drop down as close as possible to the length but the allocator
    /// may still inform the vector that there is space for a few more elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::with_capacity(10, j);
    ///     vec.extend_from_slice(&[1, 2, 3], j);
    ///     assert_eq!(vec.capacity(), 10);
    ///     vec.shrink_to_fit(j);
    ///     assert!(vec.capacity() >= 3);
    /// }).unwrap();
    /// ```
    pub fn shrink_to_fit(&mut self, j: &Journal<A>) {
        if self.capacity() != self.len {
            self.shrink_to(self.len, j);
        }
    }

    /// Copy all the elements of `other` into `Self`
    ///
    /// # Panics
    ///
    /// Panics if the number of elements in the vector overflows a `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     let mut other = vec![4, 5, 6];
    ///     vec.extend_from_slice(&other, j);
    ///     assert_eq!(vec.as_slice(), [1, 2, 3, 4, 5, 6]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn reserve(&mut self, additional: usize, j: &Journal<A>) {
        if additional == 0 {
            return;
        }

        let cap = self.buf.capacity();
        let len = self.len;
        let new_cap = len + additional.max(cap);
        if get_idx(new_cap * mem::size_of::<T>()) == get_idx(len * mem::size_of::<T>()) {
            self.buf.set_cap(new_cap);
        } else {
            unsafe {
                let old = self.as_slice_mut();
                let layout = Layout::array::<T>(new_cap).unwrap();
                let new = A::new_uninit_for_layout(layout.size(), j).cast();
                ptr::copy(old.as_ptr(), new, len);
                A::free_slice(Self::to_slice_mut(self.off(), self.capacity()));
                self.buf = FatPtr::new(slice::from_raw_parts(new, new_cap));
            }
        }
    }

    /// Shortens the vector, keeping the first `len` elements and dropping
    /// the rest.
    ///
    /// If `len` is greater than the vector's current length, this has no
    /// effect.
    ///
    /// Note that this method has no effect on the capacity. To shorten the
    /// capacity too, use [`shrink_to`] or [`shrink_to_fit`].
    ///
    /// # Examples
    ///
    /// Truncating a five element vector to three elements, then truncating it
    /// to two:
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3, 4, 5], j);
    ///
    ///     vec.truncate(3);
    ///     assert_eq!(vec, [1, 2, 3]);
    ///     assert_eq!(vec.capacity(), 5); // No effect on capacity
    ///
    ///     vec.truncate(2);
    ///     assert_eq!(vec.as_slice(), [1, 2]);
    ///     assert_eq!(vec.capacity(), 5); // Capacity is shrunk to 2
    /// }).unwrap();
    /// ```
    ///
    /// No truncation occurs when `len` is greater than the vector's current
    /// length:
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     vec.truncate(8);
    ///     assert_eq!(vec, [1, 2, 3]);
    /// }).unwrap();
    /// ```
    ///
    /// Truncating when `len == 0` is equivalent to calling the [`clear`]
    /// method.
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     vec.truncate(0);
    ///     assert_eq!(vec, []);
    /// }).unwrap();
    /// ```
    ///
    /// [`clear`]: #method.clear
    /// [`len`]: #method.len
    /// [`shrink_to`]: #method.shrink_to
    /// [`shrink_to_fit`]: #method.shrink_to_fit
    pub fn truncate(&mut self, len: usize) {
        // This is safe because:
        //
        // * the slice passed to `drop_in_place` is valid; the `len > self.len`
        //   case avoids creating an invalid slice, and
        // * the `len` of the vector is shrunk before calling `drop_in_place`,
        //   such that no value will be dropped twice in case `drop_in_place`
        //   were to panic once (if it panics twice, the program aborts).
        unsafe {
            if len > self.len {
                return;
            }

            let s = &mut self.as_slice_mut()[len..];
            ptr::drop_in_place(s);
        }
        self.len = len;
    }

    /// Removes an element from the vector and returns it.
    ///
    /// The removed element is replaced by the last element of the vector.
    ///
    /// This does not preserve ordering, but is O(1).
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut v = Vec::from_slice(&[1, 2, 3, 4], j);
    ///
    ///     assert_eq!(v.swap_remove(1), 2);
    ///     assert_eq!(v, [1, 4, 3]);
    ///
    ///     assert_eq!(v.swap_remove(0), 1);
    ///     assert_eq!(v, [3, 4]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> T {
        unsafe {
            // We replace self[index] with the last element. Note that if the
            // bounds check on hole succeeds there must be a last element (which
            // can be self[index] itself).
            let hole: *mut T = &mut self.as_slice_mut()[index];
            let last = ptr::read(self.buf.get_unchecked(self.len - 1));
            self.len -= 1;
            ptr::replace(hole, last)
        }
    }

    /// Inserts an element at position `index` within the vector, shifting all
    /// elements after it to the right.
    ///
    /// # Panics
    ///
    /// Panics if `index > len`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     vec.insert(1, 4, j);
    ///     assert_eq!(vec, [1, 4, 2, 3]);
    ///     vec.insert(4, 5, j);
    ///     assert_eq!(vec, [1, 4, 2, 3, 5]);
    /// }).unwrap();
    /// ```
    pub fn insert(&mut self, index: usize, element: T, j: &Journal<A>) {
        let len = self.len();
        assert!(index <= len);

        // space for the new element
        if len == self.buf.capacity() {
            self.reserve(1, j);
        }

        unsafe {
            // infallible
            // The spot to put the new value
            {
                let p = self.buf.as_mut_ptr().add(index);
                // Shift everything over to make space. (Duplicating the
                // `index`th element into two consecutive places.)
                ptr::copy(p, p.offset(1), len - index);
                // Write it in, overwriting the first copy of the `index`th
                // element.
                ptr::write(p, element);
            }
            self.set_len(len + 1);
        }
    }

    /// Removes and returns the element at position `index` within the vector,
    /// shifting all elements after it to the left.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut v = Vec::from_slice(&[1, 2, 3], j);
    ///     assert_eq!(v.remove(1), 2);
    ///     assert_eq!(v, [1, 3]);
    /// }).unwrap();
    /// ```
    pub fn remove(&mut self, index: usize) -> T {
        let len = self.len();
        assert!(index < len);
        unsafe {
            // infallible
            let ret;
            {
                // the place we are taking from.
                let ptr = self.buf.as_mut_ptr().add(index);
                // copy it out, unsafely having a copy of the value on
                // the stack and in the vector at the same time.
                ret = ptr::read(ptr);

                // Shift everything down to fill in that spot.
                ptr::copy(ptr.offset(1), ptr, len - index - 1);
            }
            self.set_len(len - 1);
            ret
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns `false`.
    /// This method operates in place, visiting each element exactly once in the
    /// original order, and preserves the order of the retained elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3, 4], j);
    ///     vec.retain(|&x| x % 2 == 0);
    ///     assert_eq!(vec, [2, 4]);
    /// }).unwrap();
    /// ```
    ///
    /// The exact order may be useful for tracking external state, like an index.
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3, 4, 5], j);
    ///     let keep = [false, true, true, false, true];
    ///     let mut i = 0;
    ///     vec.retain(|_| (keep[i], i += 1).0);
    ///     assert_eq!(vec, [2, 3, 5]);
    /// }).unwrap();
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let len = self.len();
        let mut del = 0;
        {
            let v = self.as_slice_mut();

            for i in 0..len {
                if !f(&v[i]) {
                    del += 1;
                } else if del > 0 {
                    v.swap(i - del, i);
                }
            }
        }
        if del > 0 {
            self.truncate(len - del);
        }
    }

    // /// Removes all but the first of consecutive elements in the vector that resolve to the same
    // /// key.
    // ///
    // /// If the vector is sorted, this removes all duplicates.
    // ///
    // /// # Examples
    // ///
    // /// ```
    // /// # use corundum::vec::Vec;
    // /// # use corundum::alloc::*;
/// Heap::transaction(|j| {
    // ///     let mut vec = Vec::from_slice(&[10, 20, 21, 30, 20], j);
    // ///
    // ///     vec.dedup_by_key(|i| *i / 10);
    // ///
    // ///     assert_eq!(vec, [10, 20, 30, 20]);
    // /// }).unwrap();
    // /// ```
    // #[inline]
    // pub fn dedup_by_key<F, K>(&mut self, mut key: F)
    // where
    //     F: FnMut(&mut T) -> K,
    //     K: PartialEq,
    // {
    //     self.dedup_by(|a, b| key(a) == key(b))
    // }

    // /// Removes all but the first of consecutive elements in the vector satisfying a given equality
    // /// relation.
    // ///
    // /// The `same_bucket` function is passed references to two elements from the vector and
    // /// must determine if the elements compare equal. The elements are passed in opposite order
    // /// from their order in the slice, so if `same_bucket(a, b)` returns `true`, `a` is removed.
    // ///
    // /// If the vector is sorted, this removes all duplicates.
    // ///
    // /// # Examples
    // ///
    // /// ```
    // /// # use corundum::vec::Vec;
    // /// # use corundum::str::*;
    // /// # use corundum::alloc::*;
/// Heap::transaction(|j| {
    // ///     let mut vec = Vec::from_slice(&["foo", "bar", "Bar", "baz", "bar"], j);
    // ///
    // ///     vec.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    // ///
    // ///     assert_eq!(vec, ["foo", "bar", "baz", "bar"]);
    // /// }).unwrap()
    // /// ```
    // pub fn dedup_by<F>(&mut self, same_bucket: F)
    // where
    //     F: FnMut(&mut T, &mut T) -> bool,
    // {
    //     let len = {
    //         let (dedup, _) = self.as_mut_slice().partition_dedup_by(same_bucket);
    //         dedup.len()
    //     };
    //     self.truncate(len);
    // }

    /// Appends an element to the back of a collection.
    ///
    /// # Panics
    ///
    /// Panics if the number of elements in the vector overflows a `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2], j);
    ///     vec.push(3, j);
    ///     assert_eq!(vec, [1, 2, 3]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn push(&mut self, value: T, j: &Journal<A>) {
        // This will panic or abort if we would allocate > isize::MAX bytes
        // or if the length increment would overflow for zero-sized types.
        if self.len == self.buf.capacity() {
            self.reserve(1, j);
        }
        unsafe {
            let end = self.buf.as_mut_ptr().add(self.len);
            ptr::write(end, value);
            self.len += 1;
        }
    }

    /// Removes the last element from a vector and returns it, or [`None`] if it
    /// is empty.
    ///
    /// [`None`]: std::option::Option::None
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     assert_eq!(vec.pop(), Some(3));
    ///     assert_eq!(vec, [1, 2]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            unsafe {
                self.len -= 1;
                Some(ptr::read(self.buf.get_unchecked(self.len())))
            }
        }
    }

    /// Moves all the elements of `other` into `Self`, leaving `other` empty.
    ///
    /// # Panics
    ///
    /// Panics if the number of elements in the vector overflows a `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut vec = Vec::from_slice(&[1, 2, 3], j);
    ///     let mut vec2 = Vec::from_slice(&[4, 5, 6], j);
    ///     vec.append(&mut vec2, j);
    ///     assert_eq!(vec, [1, 2, 3, 4, 5, 6]);
    ///     assert_eq!(vec2, []);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn append(&mut self, other: &mut Self, j: &Journal<A>) {
        unsafe {
            self.extend_from_slice(other.as_slice(), j);
            other.set_len(0);
        }
    }

    /// Clears the vector, removing all values.
    ///
    /// Note that this method has no effect on the allocated capacity
    /// of the vector.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::vec::Vec;
    /// # use corundum::alloc::*;
    /// Heap::transaction(|j| {
    ///     let mut v = Vec::from_slice(&[1, 2, 3], j);
    ///     v.clear();
    ///     assert!(v.is_empty());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    /// Drops content without logging
    #[inline]
    pub(crate) unsafe fn free_nolog(&mut self) {
        if A::valid(self) && self.capacity() > 0 {
            A::free_nolog(Self::to_slice_mut(self.off(), self.capacity()));
        }
        self.buf.set_cap(0);
        self.len = 0;
    }

    pub fn cast<U, F: Fn(&T) -> U>(&self, f: F) -> std::vec::Vec<U> {
        let mut res = std::vec::Vec::<U>::with_capacity(self.len);
        for v in self {
            res.push(f(v));
        }
        res
    }
}

impl<T: PSafe, A: MemPool> Drop for Vec<T, A> {
    fn drop(&mut self) {
        unsafe {
            let s = self.as_slice_mut();
            ptr::drop_in_place(s);
            A::free_slice(self.buf.as_slice_mut());
        }
    }
}

impl<A: MemPool, T: PSafe, I: SliceIndex<[T]>> Index<I> for Vec<T, A> {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        Index::index(&**self, index)
    }
}

// impl<A: MemPool, T: PSafe, I: SliceIndex<[T]>> IndexMut<I> for Vec<T, A> {
//     #[inline]
//     fn index_mut(&mut self, index: I) -> &mut Self::Output {
//         let self_mut = self.as_slice() as *const [T] as *mut [T];
//         IndexMut::index_mut(unsafe { &mut *self_mut }, index)
//     }
// }

// impl<T: PSafe, A: MemPool> Index<usize> for Vec<T, A> {
//     fn index(&self, i: usize) -> &T {
//         self.get(i)
//     }
// }

// impl<T: PSafe, A: MemPool> IndexMut<usize> for Vec<T, A> {
//     fn index_mut(&mut self, i: usize) -> &mut T {
//         self.get_mut(i)
//     }
// }

impl<T: PSafe, A: MemPool> std::ops::Deref for Vec<T, A> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

// impl<T: PSafe, A: MemPool> std::ops::DerefMut for Vec<T, A> {
//     fn deref_mut(&mut self) -> &mut [T] {
//         self.as_slice_mut()
//     }
// }

impl<T: PSafe + Debug, A: MemPool> Debug for Vec<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let len = self.len();
        write!(f, "[")?;
        if len > 0 {
            write!(f, "{:?}", self.get(0))?;
        }
        for i in 1..len {
            write!(f, ", {:?}", self.get(i))?;
        }
        write!(f, "]")
    }
}

impl<T: PSafe + Display, A: MemPool> Display for Vec<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let len = self.len();
        write!(f, "[")?;
        if len > 0 {
            write!(f, "{}", self.get(0))?;
        }
        for i in 1..len {
            write!(f, ", {}", self.get(i))?;
        }
        write!(f, "]")
    }
}

macro_rules! __impl_slice_eq1 {
    ([$($vars:tt)*] $lhs:ty, $rhs:ty, $($constraints:tt)*) => {
        impl<T: PSafe, U: PSafe, A: MemPool, $($vars)*> PartialEq<$rhs> for $lhs
        where
            T: PartialEq<U>,
            $($constraints)*
        {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool { self[..] == other[..] }
            #[inline]
            fn ne(&self, other: &$rhs) -> bool { self[..] != other[..] }
        }
    }
}

__impl_slice_eq1! { [] Vec<T, A>, Vec<U, A>, }
__impl_slice_eq1! { [] Vec<T, A>, &[U], }
__impl_slice_eq1! { [] Vec<T, A>, &mut [U], }
// __impl_slice_eq1! { [] Cow<'_, [T]>, &[U], T: PClone<A> }
// __impl_slice_eq1! { [] Cow<'_, [T]>, &mut [U], T: PClone<A> }
// __impl_slice_eq1! { [] Cow<'_, [T]>, Vec<U, A>, T: PClone<A> }
__impl_slice_eq1! { [const N: usize] Vec<T, A>, [U; N], }
__impl_slice_eq1! { [const N: usize] Vec<T, A>, &[U; N], }

// NOTE: some less important impls are omitted to reduce code bloat
// FIXME(Centril): Reconsider this?
// __impl_slice_eq1! { [const N: usize] Vec<A>, &mut [B; N], }
// __impl_slice_eq1! { [const N: usize] Cow<'a, [A]>, [B; N], }
// __impl_slice_eq1! { [const N: usize] Cow<'a, [A]>, &[B; N], }
// __impl_slice_eq1! { [const N: usize] Cow<'a, [A]>, &mut [B; N], }

/// Implements comparison of vectors, lexicographically.
impl<A: MemPool, T: PSafe + PartialOrd> PartialOrd for Vec<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Vec<T, A>) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
}

impl<A: MemPool, T: PSafe + PClone<A>> PClone<A> for Vec<T, A> {
    fn pclone(&self, j: &Journal<A>) -> Self {
        Vec::from_slice(PClone::pclone(&self.as_slice(), j), j)

    }
}

impl<A: MemPool, T: PSafe + Eq> Eq for Vec<T, A> {}

/// Implements ordering of vectors, lexicographically.
impl<A: MemPool, T: PSafe + Ord> Ord for Vec<T, A> {
    #[inline]
    fn cmp(&self, other: &Vec<T, A>) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<T: PSafe, A: MemPool> Default for Vec<T, A> {
    fn default() -> Self {
        Vec::empty()
    }
}

// Consuming iterator

// structure helper for consuming iterator.
pub struct IntoIteratorHelper<T> {
    iter: std::vec::IntoIter<T>,
}

// implement the IntoIterator trait for a consuming iterator. Iteration will
// consume the Words structure
impl<T: PSafe, A: MemPool> IntoIterator for Vec<T, A> {
    type Item = T;
    type IntoIter = IntoIteratorHelper<T>;

    // note that into_iter() is consuming self
    fn into_iter(self) -> Self::IntoIter {
        unsafe {
            IntoIteratorHelper {
                iter: std::vec::Vec::from_raw_parts(
                    self.buf.as_mut_ptr(),
                    self.len,
                    self.buf.capacity(),
                )
                .into_iter(),
            }
        }
    }
}

// now, implements Iterator trait for the helper struct, to be used by adapters
impl<T: PSafe> Iterator for IntoIteratorHelper<T> {
    type Item = T;

    // just return the str reference
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

// non-consuming iterator

// structure helper for non-consuming iterator.
pub struct IterHelper<'a, T> {
    iter: std::slice::Iter<'a, T>,
}

// implement the IntoIterator trait for a non-consuming iterator. Iteration will
// borrow the Words structure
impl<'a, T: PSafe, A: MemPool> IntoIterator for &'a Vec<T, A> {
    type Item = &'a T;
    type IntoIter = IterHelper<'a, T>;

    // note that into_iter() is consuming self
    fn into_iter(self) -> Self::IntoIter {
        IterHelper {
            iter: self.as_slice().iter(),
        }
    }
}

// now, implements Iterator trait for the helper struct, to be used by adapters
impl<'a, T> Iterator for IterHelper<'a, T> {
    type Item = &'a T;

    // just return the str reference
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T: PSafe, A: MemPool> AsRef<Vec<T, A>> for Vec<T, A> {
    fn as_ref(&self) -> &Vec<T, A> {
        self
    }
}

// impl<T: PSafe, A: MemPool> AsMut<Vec<T, A>> for Vec<T, A> {
//     fn as_mut(&mut self) -> &mut Vec<T, A> {
//         self
//     }
// }

impl<T: PSafe, A: MemPool> AsRef<[T]> for Vec<T, A> {
    fn as_ref(&self) -> &[T] {
        self
    }
}

// impl<T: PSafe, A: MemPool> AsMut<[T]> for Vec<T, A> {
//     fn as_mut(&mut self) -> &mut [T] {
//         self
//     }
// }

impl<T: Clone + PSafe, A: MemPool> PFrom<&[T], A> for Vec<T, A> {
    fn pfrom(s: &[T], j: &Journal<A>) -> Vec<T, A> {
        Vec::from_slice(s, j)
    }
}

impl<T: Clone + PSafe, A: MemPool> PFrom<&mut [T], A> for Vec<T, A> {
    fn pfrom(s: &mut [T], j: &Journal<A>) -> Vec<T, A> {
        Vec::from_slice(s, j)
    }
}

// impl<'a, T: PSafe, A: MemPool> From<Cow<'a, [T]>> for Vec<T, A>
// where
//     [T]: ToOwned<Owned = Vec<T, A>>,
// {
//     fn from(s: Cow<'a, [T]>) -> Vec<T, A> {
//         s.into_owned()
//     }
// }

// note: test pulls in libstd, which causes errors here
impl<T: PSafe, A: MemPool> PFrom<Box<[T]>, A> for Vec<T, A> {
    fn pfrom(s: Box<[T]>, j: &Journal<A>) -> Vec<T, A> {
        Vec::from_slice(s.into_vec().as_slice(), j)
    }
}

// // note: test pulls in libstd, which causes errors here
// impl<T: PSafe, A: MemPool> From<Pbox<[T], A>> for Vec<T, A> {
//     fn from(s: Pbox<[T], A>) -> Vec<T, A> {
//         let journal = Journal::try_current().expect("This function should be called only inside a transaction").0;
//         Vec::from_slice(s.into_vec().as_slice(), &journal)
//     }
// }

// // note: test pulls in libstd, which causes errors here
// impl<T: PSafe, A: MemPool> From<Vec<T, A>> for Pbox<[T]> {
//     fn from(v: Vec<T, A>) -> Pbox<[T]> {
//         v.into_boxed_slice()
//     }
// }

impl<A: MemPool> PFrom<&str, A> for Vec<u8, A> {
    fn pfrom(s: &str, j: &Journal<A>) -> Vec<u8, A> {
        PFrom::pfrom(s.as_bytes(), j)
    }
}

impl<A: MemPool> Vec<u8, A> {
    pub fn to_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.as_slice()) }
    }
}

#[cfg(test)]
mod test {
    use crate::default::*;

    type A = BuddyAlloc;

    #[test]
    fn test_array() {
        struct Root {
            buf: Pbox<PRefCell<PVec<i32>>>,
        }

        impl RootObj<A> for Root {
            fn init(j: &Journal) -> Self {
                Self {
                    buf: Pbox::new(PRefCell::new(PVec::default(), j), j),
                }
            }
        }

        let root = A::open::<Root>("sb4.pool", O_CFNE).unwrap();
        println!("{:?}", root.buf);

        // for _ in 0..100 {
        println!("usage: {} bytes", A::used());
        let _ = A::transaction(|j| {
            let mut buf = root.buf.borrow_mut(j);
            for _ in 0..5 {
                let i = buf.len() as i32;
                buf.extend_from_slice(&[i + 1, i + 2, i + 3], j);
            }

            // let idx_0 = buf[0];
            // buf[0] = buf[1];
            // buf[1] = buf[2];
            if crate::utils::rand() % 2 == 1 {
                panic!("test");
            }
            // buf[2] = idx_0;
        });
        println!("usage: {} bytes", A::used());
        // }

        println!("{:?}", root.buf);
    }

    #[test]
    fn test_resize() {
        struct Root {
            buf: Pbox<PRefCell<PVec<i32>>>,
        }

        impl RootObj<A> for Root {
            fn init(j: &Journal) -> Self {
                Self {
                    buf: Pbox::new(PRefCell::new(PVec::empty(), j), j),
                }
            }
        }

        let root = A::open::<Root>("sb5.pool", O_CFNE).unwrap();
        println!("PRE: {:?}", root.buf);

        let pre = A::used();
        let _ = A::transaction(|j| {
            let mut buf = root.buf.borrow_mut(j);
            // for _ in 0..5 {
            let i = buf.len() as i32;
            buf.extend_from_slice(&[i + 1], j);
            // panic!("intentional");
            // std::process::exit(0);
            // }
        });

        println!("NEXT: {:?}", root.buf);

        println!(" pre usage = {} bytes", pre);
        println!("post usage = {} bytes", A::used());
    }

    #[test]
    fn test_clear() {
        use crate::vec::Vec;
        Heap::transaction::<_, _>(|j| {
            let mut vec = Vec::from_slice(&[1, 2, 3], j);
            vec.truncate(0);
            assert_eq!(vec, []);
        })
        .unwrap();
    }
}
