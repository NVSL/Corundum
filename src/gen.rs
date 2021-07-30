use core::ffi::c_void;
use crate::stm::Journal;
use crate::clone::PClone;
use crate::alloc::*;
use crate::vec::Vec as PVec;

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

    pub fn new(bytes: &[u8], j: &Journal<P>) -> Self {
        Self { bytes: PVec::from_slice(bytes, j) }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.bytes.as_slice().to_vec()
    }

    pub fn ptr(&self) -> *const c_void {
        self.bytes.as_slice().as_ptr() as *const c_void
    }

    pub fn ptr_mut(&mut self) -> *mut c_void {
        self.bytes.as_slice().as_ptr() as *mut c_void
    }

    pub fn to_ptr_mut(slf: &mut Self) -> *mut c_void {
        slf.bytes.as_slice().as_ptr() as *mut c_void
    }
}