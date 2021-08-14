#![cfg(feature = "cbindings")]

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ffi::c_void;
use std::panic::{UnwindSafe, RefUnwindSafe};
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
        Self::new(vec![0; size].as_slice(), j)
    }

    pub fn from_bytes(bytes: &[u8], j: &Journal<P>) -> Self {
        Self { bytes: PVec::from_slice(bytes, j) }
    }

    pub unsafe fn from_raw(ptr: *const c_void, len: usize) -> Self {
        Self {
            bytes: PVec::from_raw_parts(ptr as *mut u8, len, len)
        }
    }

    pub fn new<T: PSafe + ?Sized>(obj: &T, j: &Journal<P>) -> Self {
        Self { bytes: PVec::from_slice(utils::as_slice(obj), j) }
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

    pub fn as_gen<T>(&self) -> Gen<T, P> {
        Gen::<T, P>::from(self.as_ptr::<T>())
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
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> Gen<T, P> {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *mut u8, self.len) }
    }
}

impl<T, P: MemPool> From<&c_void> for Gen<T, P> {
    fn from(obj: &c_void) -> Self {
        Self {
            ptr: obj as *const c_void,
            len: std::mem::size_of::<T>(),
            phantom: PhantomData
        }
    }
}

impl<'a, T, P: MemPool> Into<&'a c_void> for Gen<T, P> {
    fn into(self) -> &'a c_void {
        unsafe { &*self.ptr }
    }
}

impl<T, P: MemPool> From<*const T> for Gen<T, P> {
    fn from(obj: *const T) -> Self {
        Self {
            ptr: obj as *const T as *const c_void,
            len: std::mem::size_of::<T>(),
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> From<&[u8]> for Gen<T, P> {
    fn from(bytes: &[u8]) -> Self {
        Self {
            ptr: bytes.as_ref().as_ptr() as *const c_void,
            len: bytes.len(),
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> From<ByteObject<P>> for Gen<T, P> {
    fn from(obj: ByteObject<P>) -> Self {
        Self {
            ptr: obj.as_ptr(),
            len: obj.len(),
            phantom: PhantomData
        }
    }
}
