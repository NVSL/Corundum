#![cfg(feature = "cbindings")]

// use std::hash::*;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ffi::c_void;
use std::panic::{UnwindSafe, RefUnwindSafe};
use std::mem::size_of;
use crate::*;
use crate::stm::Journal;
use crate::clone::PClone;
use crate::alloc::*;
use crate::ptr::*;
use crate::stm::{Logger,Notifier};

#[repr(C)]
pub struct Gen<T, P: MemPool> {
    ptr: *const c_void,
    len: usize,
    phantom: PhantomData<(T,P)>
}

unsafe impl<T, P: MemPool> TxInSafe for Gen<T, P> {}
unsafe impl<T, P: MemPool> LooseTxInUnsafe for Gen<T, P> {}
impl<T, P: MemPool> UnwindSafe for Gen<T, P> {}
impl<T, P: MemPool> RefUnwindSafe for Gen<T, P> {}

/// A byte-vector representation of any type
/// 
/// It is useful for FFI functions when template types cannot be externally used.
/// 
/// # Examples
/// 
/// ```
/// corundum::pool!(pool);
/// use pool::*;
/// type P = Allocator;
/// 
/// use corundum::gen::{ByteArray,Gen};
/// 
/// struct ExternalType {
///     obj: ByteArray<P>
/// }
/// 
/// #[no_mangle]
/// pub extern "C" fn new_obj(obj: Gen) {
///     
/// }
/// ```
#[derive(Clone)]
pub struct ByteArray<P: MemPool> {
    bytes: Slice<u8, P>,
    logged: u8
}

// impl<P: MemPool> Copy for ByteArray<P> {}

unsafe impl<P: MemPool> PSafe for ByteArray<P> {}
unsafe impl<P: MemPool> LooseTxInUnsafe for ByteArray<P> {}
impl<P: MemPool> UnwindSafe for ByteArray<P> {}
impl<P: MemPool> RefUnwindSafe for ByteArray<P> {}

impl<P: MemPool> Default for ByteArray<P> {
    fn default() -> Self {
        Self {
            bytes: Default::default(),
            logged: 0
        }
    }
}

impl<P: MemPool> PClone<P> for ByteArray<P> {
    fn pclone(&self, j: &Journal<P>) -> Self {
        Self {
            bytes: self.bytes.pclone(j),
            logged: 0
        }
    }
}

impl<P: MemPool> Drop for ByteArray<P> {
    fn drop(&mut self) {
        unsafe {
            if !self.bytes.is_empty() {
                P::dealloc(self.bytes.as_mut_ptr(), self.bytes.capacity())
            }
        }
    }
}

// impl<P: MemPool> Hash for ByteArray<P> {
//     fn hash<H>(&self, hasher: &mut H) where H: Hasher {
//         self.bytes.as_slice().hash(hasher)
//     }
// }

// impl<P: MemPool> PartialEq for ByteArray<P> {
//     fn eq(&self, other: &Self) -> bool {
//         self.bytes.as_slice().eq(other.bytes.as_slice())
//     }
// }

// impl<T,P: MemPool> PartialEq<Gen<T,P>> for ByteArray<P> {
//     fn eq(&self, other: &Gen<T,P>) -> bool {
//         self.bytes.as_slice().eq(other.as_slice())
//     }
// }

// impl<P: MemPool + Eq> PartialOrd for ByteArray<P> {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         self.bytes.as_slice().partial_cmp(other.bytes.as_slice())
//     }
// }

// impl<T, P: MemPool + Eq> PartialOrd<Gen<T,P>> for ByteArray<P> {
//     fn partial_cmp(&self, other: &Gen<T,P>) -> Option<std::cmp::Ordering> {
//         self.bytes.as_slice().partial_cmp(other.as_slice())
//     }
// }

// impl<P: MemPool + Eq> Ord for ByteArray<P> {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         self.bytes.as_slice().cmp(other.bytes.as_slice())
//     }
// }

// impl<T, P: MemPool> Hash for Gen<T, P> {
//     fn hash<H>(&self, hasher: &mut H) where H: Hasher {
//         self.as_slice().hash(hasher)
//     }
// }

impl<P: MemPool> ByteArray<P> {
    pub unsafe fn alloc(size: usize, j: &Journal<P>) -> Self {
        let ptr = P::new_uninit_for_layout(size, j);
        Self { bytes: Slice::from_raw_parts(ptr, size), logged: 0 }
    }

    pub fn null() -> Self {
        Self {
            bytes: Default::default(),
            logged: 0
        }
    }

    pub fn as_ref<T>(&self) -> &T {
        unsafe { &*(self.bytes.as_ptr() as *const T) }
    }

    fn from_gen<T>(obj: Gen<T, P>) -> Self {
        Self { 
            bytes: unsafe { Slice::from_raw_parts(obj.ptr as *const u8, obj.len) }, 
            logged: 0 
        }
    }

    /// Retrieves an unsafe `Gen` sharing the same pointer and leaks the allocation
    /// 
    /// # Safety
    /// The returned `Gen` shares the same pointer, but does not drop it. 
    /// Accessing data through the returned `Gen` may have undefined behavior. 
    pub unsafe fn leak<T>(self) -> Gen<T, P> {
        Gen::from_byte_object(self)
    }

    /// Retrieves an unsafe `Gen` sharing the same pointer
    /// 
    /// # Safety
    /// The returned `Gen` shares the same pointer, but does not drop it. 
    /// Accessing data through the returned `Gen` may have undefined behavior. 
    pub unsafe fn get_gen<T>(&self) -> Gen<T, P> {
        // assert_eq!(self.len(), size_of::<T>(), "Incompatible type casting");
        Gen::<T, P>::from_ptr(self.get_ptr::<T>())
    }

    pub unsafe fn as_mut<T>(&self) -> &mut T {
        &mut *(self.bytes.as_ptr() as *mut T)
    }

    pub fn get_ptr<T>(&self) -> *const T {
        self.bytes.as_ptr() as *const T
    }

    pub fn get_ptr_mut(&mut self) -> *mut c_void {
        self.bytes.as_ptr() as *mut c_void
    }

    pub unsafe fn to_ptr_mut(slf: &mut Self) -> *mut c_void {
        slf.bytes.as_ptr() as *mut c_void
    }

    pub fn off(&self) -> u64 {
        self.bytes.off()
    }

    pub fn write_to<T>(&self, loc: &mut MaybeUninit<T>) {
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.bytes.as_ptr(), 
                loc as *mut _ as *mut u8, 
                self.bytes.capacity());
        }
    }

    /// Swaps the contents of two `ByteArray`s
    pub fn swap(&mut self, other: &mut Self) {
        let slice = self.bytes;
        self.bytes = other.bytes;
        other.bytes = slice;
    }

    pub fn len(&self) -> usize {
        self.bytes.capacity()
    }

    pub fn update_from_gen<T>(&self, new: Gen<T, P>, j: &Journal<P>) {
        unsafe {
            let slice = utils::as_mut(self).bytes.as_slice_mut();
            if self.logged == 0 {
                slice.create_log(j, Notifier::NonAtomic(Ptr::from_ref(&self.logged)));
            }
            std::ptr::copy_nonoverlapping(new.ptr, slice as *mut [u8] as *mut c_void, slice.len())
        }
    }
}

impl<T, P: MemPool> From<Gen<T, P>> for ByteArray<P> {
    fn from(g: Gen<T, P>) -> Self {
        Self::from_gen(g)
    }
}

impl<T, P: MemPool> Gen<T, P> {
    pub fn null() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
            // drop: false,
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> Gen<T, P> {
    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *mut u8, self.len) }
    }

    fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut u8, self.len) }
    }

    fn from_ptr(obj: *const T) -> Self {
        Self {
            ptr: obj as *const T as *const c_void,
            len: size_of::<T>(),
            // drop: false,
            phantom: PhantomData
        }
    }

    fn from_byte_object(obj: ByteArray<P>) -> Self {
        // assert_eq!(obj.len(), size_of::<T>(), "Incompatible type casting");
        Self {
            ptr: obj.get_ptr(),
            len: obj.len(),
            phantom: PhantomData
        }
    }

    pub fn as_ref(&self) -> &T {
        unsafe { crate::utils::read(self.ptr as *mut u8) }
    }

    pub fn ptr(&self) -> *const c_void {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;

//     impl<T, P: MemPool> From<&T> for Gen<T, P> {
//         fn from(obj: &T) -> Self {
//             Self {
//                 ptr: obj as *const T as *const c_void,
//                 len: size_of::<T>(),
//                 phantom: PhantomData
//             }
//         }
//     }
// }