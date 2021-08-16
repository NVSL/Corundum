#![cfg(feature = "cbindings")]

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ffi::c_void;
use std::panic::{UnwindSafe, RefUnwindSafe};
use std::mem::size_of;
use crate::*;
use crate::stm::Journal;
use crate::clone::PClone;
use crate::alloc::*;
use crate::vec::Vec as PVec;

#[repr(C)]
pub struct Gen<T, P: MemPool> {
    pub ptr: *const c_void,
    pub len: usize,
    phantom: PhantomData<(T,P)>
}

unsafe impl<T: TxInSafe, P: MemPool> TxInSafe for Gen<T, P> {}
unsafe impl<T: LooseTxInUnsafe, P: MemPool> LooseTxInUnsafe for Gen<T, P> {}
impl<T: TxInSafe, P: MemPool> UnwindSafe for Gen<T, P> {}
impl<T: TxInSafe, P: MemPool> RefUnwindSafe for Gen<T, P> {}

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
/// use corundum::gen::{ByteObject,Gen};
/// 
/// struct ExternalType {
///     obj: ByteObject<P>
/// }
/// 
/// #[no_mangle]
/// pub extern "C" fn new_obj(obj: Gen) {
///     
/// }
/// ```
pub struct ByteObject<P: MemPool> {
    bytes: PVec<u8, P>
}

impl<P: MemPool> PClone<P> for ByteObject<P> {
    fn pclone(&self, j: &Journal<P>) -> Self {
        Self {
            bytes: self.bytes.pclone(j)
        }
    }
}

impl<P: MemPool> ByteObject<P> {
    pub fn new_uninit(size: usize, j: &Journal<P>) -> Self {
        // Self::new(vec![0; size].as_slice(), j)
        Self { bytes: PVec::from_slice(vec![0; size].as_slice(), j) }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    pub fn as_ref<T>(&self) -> &T {
        unsafe { &*(self.bytes.as_ptr() as *const T) }
    }

    pub fn from_gen<T>(obj: Gen<T, P>, j: &Journal<P>) -> Self {
        let bytes = obj.as_slice();
        Self { bytes: PVec::from_slice(bytes, j) }
    }

    pub fn as_gen<T>(self) -> Gen<T, P> {
        Gen::from_byte_object(self)
    }

    pub unsafe fn from_ref_gen<T>(mut obj: Gen<T, P>) -> Self {
        let bytes = obj.as_slice_mut();
        Self { bytes: PVec::from_raw_parts(bytes.as_mut_ptr(), bytes.len(), bytes.len()) }
    }

    pub unsafe fn as_ref_gen<T>(&self) -> Gen<T, P> {
        // assert_eq!(self.len(), size_of::<T>(), "Incompatible type casting");
        Gen::<T, P>::from_ptr(self.as_ptr::<T>())
    }

    pub unsafe fn as_mut<T>(&self) -> &mut T {
        &mut *(self.bytes.as_ptr() as *mut T)
    }

    pub fn as_ptr<T>(&self) -> *const T {
        unsafe { self.bytes.as_ptr() as *const T }
    }

    pub fn as_ptr_mut(&mut self) -> *mut c_void {
        unsafe { self.bytes.as_ptr() as *mut c_void }
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
                self.bytes.len());
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
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

    fn from_byte_object(obj: ByteObject<P>) -> Self {
        // assert_eq!(obj.len(), size_of::<T>(), "Incompatible type casting");
        Self {
            ptr: obj.as_ptr(),
            len: obj.len(),
            phantom: PhantomData
        }
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