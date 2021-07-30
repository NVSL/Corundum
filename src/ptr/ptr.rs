use crate::alloc::MemPool;
use crate::alloc::PmemUsage;
use crate::{PSafe, TxOutSafe};
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::ops::*;
use std::ptr::NonNull;

#[derive(Eq)]
/// A wrapper around a raw persistent pointer that indicates that the possessor
/// of this wrapper owns the referent. Useful for building abstractions like
/// [`Pbox<T,P>`], [`Vec<T,P>`], and [`String<P>`].
///
/// Just like raw pointers, it contains the address of the object, but the
/// address is static within a file.
///
/// Note that, memory pools are types, not objects. For more information,
/// please see [`MemPool`](../alloc/struct.MemPool.html).
///
/// [`Pbox<T,P>`]: ../boxed/struct.Pbox.html
/// [`Vec<T,P>`]: ../vec/struct.Vec.html
/// [`String<P>`]: ../str/struct.String.html
/// 
pub struct Ptr<T: ?Sized, A: MemPool> {
    off: u64,
    marker: PhantomData<(A, T)>,
}

/// `Ptr` pointers are not `Send` because the data they reference may be aliased.
// N.B., this impl is unnecessary, but should provide better error messages.
impl<A: MemPool, T> !Send for Ptr<T, A> {}

/// `Ptr` pointers are not `Sync` because the data they reference may be aliased.
// N.B., this impl is unnecessary, but should provide better error messages.
impl<A: MemPool, T> !Sync for Ptr<T, A> {}
impl<A: MemPool, T> !TxOutSafe for Ptr<T, A> {}

/// The allocator does not need to implement `PSafe`
unsafe impl<A: MemPool, T: PSafe + ?Sized> PSafe for Ptr<T, A> {}

impl<A: MemPool, T: ?Sized> Ptr<T, A> {
    #[inline]
    /// Gives a reference to the inner value if it is not dangling, otherwise, None.
    pub(crate) fn try_deref(&self) -> Option<&T> {
        if self.is_dangling() {
            None
        } else {
            Some(self.as_ref())
        }
    }

    #[inline]
    /// Gives a mutable reference to the inner value if it is not dangling, otherwise, None.
    pub(crate) fn try_deref_mut(&mut self) -> Option<&mut T> {
        if self.is_dangling() {
            None
        } else {
            Some(self.as_mut())
        }
    }
}

impl<A: MemPool, T: ?Sized> Ptr<T, A> {
    #[inline]
    /// Creates new `Ptr` if `p` is valid
    pub(crate) fn new(p: &T) -> Option<Ptr<T, A>> {
        if let Ok(off) = A::off(p) {
            Some(Self {
                off,
                marker: PhantomData,
            })
        } else {
            None
        }
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
    pub(crate) fn as_mut_ptr(&mut self) -> *mut T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    #[inline]
    /// Returns the mutable raw pointer of the value
    pub(crate) fn get_mut_ptr(&self) -> *mut T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    #[inline]
    /// Returns the mutable raw pointer of the value
    pub(crate) fn as_ptr(&self) -> *const T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    #[inline]
    #[allow(clippy::mut_from_ref)]
    /// Returns the mutable reference of the value
    pub(crate) fn get_mut(&self) -> &mut T {
        unsafe { A::get_mut_unchecked(self.off) }
    }

    /// Creates a new copy of data and returns a `Ptr` pointer
    ///
    /// # Safety
    ///
    /// The compiler would not drop the copied data. Developer has the
    /// responsibility of deallocating inner value. Also, it does not clone the
    /// inner value. Instead, it just copies the memory.
    pub(crate) unsafe fn dup(&self) -> Ptr<T, A> {
        let src = self.as_ref();
        let dst = A::alloc_for_value(src);
        let trg = A::off_unchecked(dst);
        let len = std::alloc::Layout::for_value(src).size();
        std::ptr::copy_nonoverlapping(
            src as *const T as *const u8,
            dst as *mut T as *mut u8,
            len,
        );
        Ptr::from_off_unchecked(trg)
    }

    #[inline]
    /// Checks if this pointer is dangling
    pub fn is_dangling(&self) -> bool {
        self.off == u64::MAX
    }

    #[inline]
    /// Returns the file offset
    pub fn off(&self) -> u64 {
        self.off
    }


    #[inline]
    /// Returns a reference to the file offset
    pub fn off_ref(&self) -> &u64 {
        &self.off
    }

    #[inline]
    /// Returns a reference to the file offset
    pub(crate) fn off_mut(&self) -> &mut u64 {
        unsafe { &mut *(&self.off as *const u64 as *mut u64) }
    }

    #[inline]
    pub(crate) fn replace(&self, new: u64) -> u64 {
        let old = self.off;
        *self.off_mut() = new;
        old
    }

    /// Creates a new `Ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null and in the valid address range.
    #[inline]
    pub(crate) unsafe fn new_unchecked(ptr: *const T) -> Self {
        Self {
            off: A::off_unchecked(ptr),
            marker: PhantomData,
        }
    }

    #[inline]
    /// Creates a new `Ptr` that is dangling.
    pub(crate) const fn dangling() -> Ptr<T, A> {
        Self {
            off: u64::MAX,
            marker: PhantomData,
        }
    }

    #[inline]
    /// Creates new `Ptr` from file offset and len
    ///
    /// # Safety
    ///
    /// `off` should be valid
    pub(crate) const unsafe fn from_off_unchecked(off: u64) -> Ptr<T, A> {
        Self {
            off,
            marker: PhantomData,
        }
    }

    #[inline]
    #[track_caller]
    /// Consumes self as converts in to `Option<Self>` considering it whether
    /// points to a valid address or not.
    pub(crate) fn as_option(&mut self) -> Option<&mut Self> {
        if self.is_dangling() {
            None
        } else {
            debug_assert!(A::allocated(self.off(), 1), "Access Violation at address 0x{:x}", self.off());
            Some(self)
        }
    }

    #[inline]
    /// Instantiates a `Ptr` pointer. It is dangling if input is None.
    pub(crate) fn from_option<U: PSafe>(p: Option<Self>) -> Ptr<U, A> {
        if let Some(p) = p {
            Ptr {
                off: p.off,
                marker: PhantomData,
            }
        } else {
            Ptr::<U, A>::dangling()
        }
    }

    #[inline]
    /// Creates a `Ptr` type if the input `off` is valid
    pub(crate) fn try_from<'a, U: 'a + PSafe>(off: u64) -> Option<Ptr<U, A>> {
        if off == u64::MAX || !A::contains(off) {
            None
        } else {
            unsafe { Some(Ptr::<U, A>::from_off_unchecked(off)) }
        }
    }

    #[inline]
    /// Casts to a pointer of another type.
    pub(crate) fn cast<U: PSafe>(&self) -> Ptr<U, A> {
        Ptr::<U, A> {
            off: self.off,
            marker: PhantomData,
        }
    }
}

// impl<A: MemPool, T: PSafe + ?Sized> MemLayout for Ptr<T, A> {
//     fn layout(&self) -> Layout {
//         Layout::new::<32>()
//     }
// }

unsafe impl<A: MemPool, T: PSafe> PSafe for Ptr<[T], A> {}

impl<A: MemPool, T: PSafe + ?Sized> Copy for Ptr<T, A> {}

impl<A: MemPool, T: PSafe + ?Sized> Clone for Ptr<T, A> {
    fn clone(&self) -> Self {
        Self {
            off: self.off,
            marker: PhantomData,
        }
    }
}

impl<A: MemPool, T: PSafe> PmemUsage for Ptr<T, A> {
    fn size_of() -> usize {
        std::mem::size_of::<T>() + std::mem::size_of::<Self>()
    }
}

impl<A: MemPool, T: ?Sized> Ptr<T, A> {    
    #[inline]
    #[track_caller]
    pub unsafe fn from_raw(other: *const T) -> Self {
        let off = if !other.is_null() {
            A::off(other).unwrap()
        } else {
            0
        };
        Self {
            off,
            marker: PhantomData,
        }
    }
    
    #[inline]
    #[track_caller]
    pub(crate) fn from_ref(other: &T) -> Self {
        Self {
            off: A::off(other as *const T).unwrap(),
            marker: PhantomData,
        }
    }
    
    #[inline]
    #[track_caller]
    pub(crate) fn from_mut(other: &mut T) -> Self {
        Self {
            off: A::off(other as *const T).unwrap(),
            marker: PhantomData,
        }
    }
    
    #[inline]
    #[track_caller]
    pub(crate) fn from_non_null(other: NonNull<T>) -> Self {
        Self {
            off: A::off(other.as_ptr()).unwrap(),
            marker: PhantomData,
        }
    }
}

// impl<A: MemPool + Eq, T: Ord + PSafe + ?Sized> Ord for Ptr<T, A> {
//     #[inline]
//     fn cmp(&self, other: &Self) -> Ordering {
//         self.off.cmp(&other.off)
//     }
// }

// impl<A: MemPool, T: PSafe + ?Sized> PartialOrd for Ptr<T, A> {
//     #[inline]
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         Some(self.off.cmp(&other.off))
//     }
// }

impl<A: MemPool, T: ?Sized> PartialEq for Ptr<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.off == other.off
    }
}

// impl<A: MemPool, T: PSafe> PartialOrd<u64> for Ptr<T, A> {
//     #[inline]
//     fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
//         Some(self.off.cmp(&other))
//     }
// }

// impl<A: MemPool, T: PSafe> PartialEq<u64> for Ptr<T, A> {
//     #[inline]
//     fn eq(&self, other: &u64) -> bool {
//         self.off == *other
//     }
// }

impl<A: MemPool, T: PSafe + ?Sized> Deref for Ptr<T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<A: MemPool, T: PSafe + ?Sized> DerefMut for Ptr<T, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<A: MemPool, T: Debug + PSafe + ?Sized> Debug for Ptr<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}

impl<A: MemPool, T: Display + PSafe + ?Sized> Display for Ptr<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}
