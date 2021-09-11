#![cfg(feature = "cbindings")]

// use std::hash::*;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::panic::{UnwindSafe, RefUnwindSafe};
use std::mem::size_of;
use crate::*;
use crate::stm::Journal;
use crate::clone::PClone;
use crate::alloc::*;
use crate::ptr::*;
use crate::stm::{Logger,Notifier};

pub static mut CODE_SEGMENT_BASE: i64 = 0;

#[repr(C)]
pub struct Gen<T, P: MemPool> {
    ptr: *const c_void,
    len: usize,
    destructor_address: i64,
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
pub struct ByteArray<T, P: MemPool> {
    bytes: Slice<u8, P>,
    destructor_address: i64,
    logged: u8,
    phantom: PhantomData<T>
}

// impl<P: MemPool> Copy for ByteArray<P> {}

// unsafe impl<T, P: MemPool> PSafe for ByteArray<T, P> {}
unsafe impl<T, P: MemPool> LooseTxInUnsafe for ByteArray<T, P> {}
impl<T, P: MemPool> UnwindSafe for ByteArray<T, P> {}
impl<T, P: MemPool> RefUnwindSafe for ByteArray<T, P> {}

impl<T, P: MemPool> Default for ByteArray<T, P> {
    fn default() -> Self {
        Self {
            bytes: Default::default(),
            destructor_address: 0,
            logged: 0,
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> PClone<P> for ByteArray<T, P> {
    fn pclone(&self, j: &Journal<P>) -> Self {
        Self {
            bytes: self.bytes.pclone(j),
            destructor_address: self.destructor_address,
            logged: 0,
            phantom: PhantomData
        }
    }
}

impl<T, P: MemPool> Drop for ByteArray<T, P> {
    fn drop(&mut self) {
        unsafe {
            if !self.bytes.is_empty() {
                let ptr = self.bytes.as_mut_ptr();
                if self.destructor_address != 0 {
                    let addr = self.destructor_address+CODE_SEGMENT_BASE;
                    union U {
                        addr: i64,
                        drop: extern "C" fn(*mut c_void)->()
                    }
                    (U {addr}.drop)(ptr as *mut c_void);
                }
                P::dealloc(ptr, self.bytes.capacity())
            }
        }
    }
}

impl<T, P: MemPool> ByteArray<T, P> {
    pub unsafe fn alloc(size: usize, j: &Journal<P>) -> Self {
        let ptr = P::new_uninit_for_layout(size, j);
        Self { 
            bytes: Slice::from_raw_parts(ptr, size), 
            destructor_address: 0,
            logged: 0,
            phantom: PhantomData
        }
    }

    pub fn null() -> Self {
        Self {
            bytes: Default::default(),
            destructor_address: 0,
            logged: 0,
            phantom: PhantomData
        }
    }

    pub fn as_ref(&self) -> &T {
        unsafe { &*(self.bytes.as_ptr() as *const T) }
    }

    fn from_gen(obj: Gen<T, P>) -> Self {
        Self { 
            bytes: unsafe { Slice::from_raw_parts(obj.ptr as *const u8, obj.len) }, 
            destructor_address: obj.destructor_address,
            logged: 0,
            phantom: PhantomData
        }
    }

    /// Retrieves an unsafe `Gen` sharing the same pointer and leaks the allocation
    /// 
    /// # Safety
    /// The returned `Gen` shares the same pointer, but does not drop it. 
    /// Accessing data through the returned `Gen` may have undefined behavior. 
    pub unsafe fn leak(self) -> Gen<T, P> {
        Gen::from_byte_object(self)
    }

    /// Retrieves an unsafe `Gen` sharing the same pointer
    /// 
    /// # Safety
    /// The returned `Gen` shares the same pointer, but does not drop it. 
    /// Accessing data through the returned `Gen` may have undefined behavior. 
    pub unsafe fn get_gen(&self) -> Gen<T, P> {
        // assert_eq!(self.len(), size_of::<T>(), "Incompatible type casting");
        Gen::<T, P>::from_ptr(self.get_ptr())
    }

    pub fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.bytes.as_ptr() as *mut T) }
    }

    pub unsafe fn get_mut(&self) -> &mut T {
        &mut *(self.bytes.as_ptr() as *mut T)
    }

    pub fn get_ptr(&self) -> *const T {
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

    pub fn write_to(&self, loc: &mut MaybeUninit<T>) {
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

    pub fn update_from_gen(&self, new: Gen<T, P>, j: &Journal<P>) {
        unsafe {
            let slice = utils::as_mut(self).bytes.as_slice_mut();
            if self.logged == 0 {
                slice.create_log(j, Notifier::NonAtomic(Ptr::from_ref(&self.logged)));
            }
            std::ptr::copy_nonoverlapping(new.ptr, slice as *mut [u8] as *mut c_void, slice.len())
        }
    }
}

impl<T, P: MemPool> From<Gen<T, P>> for ByteArray<T, P> {
    fn from(g: Gen<T, P>) -> Self {
        Self::from_gen(g)
    }
}

impl<T, P: MemPool> Gen<T, P> {
    pub fn null() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
            destructor_address: 0,
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
            destructor_address: 0,
            phantom: PhantomData
        }
    }

    fn from_byte_object(obj: ByteArray<T, P>) -> Self {
        let res = Self {
            ptr: obj.bytes.as_ptr() as *const c_void,
            len: obj.len(),
            destructor_address: obj.destructor_address,
            phantom: PhantomData
        };
        std::mem::forget(obj);
        res
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
