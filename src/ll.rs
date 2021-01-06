//! Low-level utils
 
#![allow(unused)]

#[inline]
pub fn cpu() -> usize {
    std::thread::current().id().as_u64().get() as usize
}

#[cfg(target_arch = "x86")]
use std::arch::x86::{_mm_mfence, _mm_sfence, clflush};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_clflush, _mm_mfence, _mm_sfence};

/// Synchronize caches and memories and acts like a write barrier
#[inline]
pub fn msync<T: ?Sized>(ptr: &T, len: usize) {
    #[cfg(feature = "perf_stat")]
    let _perf = crate::stat::Measure::<crate::default::BuddyAlloc>::Sync(std::time::Instant::now());

    #[cfg(not(feature = "no_persist"))]
    {
        #[cfg(not(feature = "use_msync"))]
        clflush(ptr, len);

        #[cfg(feature = "use_msync")]
        unsafe {
            let off = ptr as usize;
            let end = off + len;
            let off = (off >> 12) << 12;
            let len = end - off;
            let ptr = off as *const u8;
            if libc::msync(
                ptr as *mut libc::c_void,
                len,
                libc::MS_SYNC | libc::MS_INVALIDATE,
            ) != 0
            {
                panic!("msync failed");
            }
        }
    }
}

/// Synchronize caches and memories and acts like a write barrier
#[inline]
pub fn msync_obj<T: ?Sized>(obj: &T) {
    #[cfg(feature = "perf_stat")]
    let _perf = crate::stat::Measure::<crate::default::BuddyAlloc>::Sync(std::time::Instant::now());

    #[cfg(not(feature = "no_persist"))]
    {
        #[cfg(not(feature = "use_msync"))]
        clflush_obj(obj);

        #[cfg(feature = "use_msync")]
        {
            let ptr = obj as *const T as *const u8 as *mut u8;
            let len = std::mem::size_of_val(obj);
            msync(ptr, len);
        }
    }
}

/// Flushes cache line back to memory
#[inline]
pub fn clflush<T: ?Sized>(ptr: &T, len: usize) {
    #[cfg(not(feature = "no_persist"))]
    {
        let ptr = ptr as *const _ as *const u8 as *mut u8;
        let mut start = ptr as usize;
        start = (start >> 9) << 9;
        let end = start + len;

        #[cfg(feature = "display_all_flushes")]
        println!("flush {:x} ({})", start, len);

        while start < end {
            unsafe {
                #[cfg(not(any(feature = "use_clflushopt", feature = "use_clwb")))]
                {
                    llvm_asm!("clflush ($0)" :: "r"(start as *const u8));
                }
                #[cfg(all(feature = "use_clflushopt", not(feature = "use_clwb")))]
                {
                    llvm_asm!("clflushopt ($0)" :: "r"(start as *const u8));
                    #[cfg(feature = "use_sfence")]
                    {
                        llvm_asm!("sfence");
                    }
                }
                #[cfg(all(feature = "use_clwb", not(feature = "use_clflushopt")))]
                {
                    llvm_asm!("clwb ($0)" :: "r"(start as *const u8));
                    #[cfg(feature = "use_sfence")]
                    {
                        llvm_asm!("sfence");
                    }
                }
                #[cfg(all(feature = "use_clwb", feature = "use_clflushopt"))]
                {
                    compile_error!("Please Select only one from clflushopt and clwb")
                }
            }
            start += 64;
        }
    }
}

/// Flushes cache lines of a whole object back to memory
#[inline]
pub fn clflush_obj<T: ?Sized>(obj: &T) {
    #[cfg(not(feature = "no_persist"))]
    {
        let len = std::mem::size_of_val(obj);
        clflush(obj, len)
    }
}

/// Store fence
#[inline]
pub fn sfence() {
    unsafe {
        _mm_sfence();
    }
}

/// Memory fence
#[inline]
pub fn mfence() {
    unsafe {
        _mm_mfence();
    }
}
