//! Low-level utils
 
#![allow(unused)]

#[inline(always)]
pub fn cpu() -> usize {
    std::thread::current().id().as_u64().get() as usize
}

#[cfg(target_arch = "x86")]
use std::arch::x86::{_mm_mfence, _mm_sfence, clflush};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_clflush, _mm_mfence, _mm_sfence};

/// Synchronize caches and memories and acts like a write barrier
#[inline(always)]
pub fn persist<T: ?Sized>(ptr: &T, len: usize, fence: bool) {
    #[cfg(feature = "stat_perf")]
    let _perf = crate::stat::Measure::<crate::default::BuddyAlloc>::Sync(std::time::Instant::now());

    #[cfg(not(feature = "no_persist"))]
    {
        #[cfg(not(feature = "use_msync"))]
        clflush(ptr, len, fence);

        #[cfg(feature = "use_msync")]
        unsafe {
            let off = ptr as *const T as *const u8 as usize;
            let end = off + len;
            let off = (off >> 12) << 12;
            let len = end - off;
            let ptr = off as *const u8;
            if libc::persist(
                ptr as *mut libc::c_void,
                len,
                libc::MS_SYNC | libc::MS_INVALIDATE,
            ) != 0
            {
                panic!("persist failed");
            }
        }
    }
}

/// Synchronize caches and memories and acts like a write barrier
#[inline(always)]
pub fn persist_obj<T: ?Sized>(obj: &T, fence: bool) {
    #[cfg(feature = "stat_perf")]
    let _perf = crate::stat::Measure::<crate::default::BuddyAlloc>::Sync(std::time::Instant::now());

    #[cfg(not(feature = "no_persist"))]
    {
        #[cfg(not(feature = "use_msync"))]
        clflush_obj(obj, fence);

        #[cfg(feature = "use_msync")]
        {
            persist(obj, std::mem::size_of_val(obj));
        }
    }
}

/// Flushes cache line back to memory
#[inline(always)]
pub fn clflush<T: ?Sized>(ptr: &T, len: usize, fence: bool) {
    #[cfg(not(feature = "no_persist"))]
    {
        let ptr = ptr as *const _ as *const u8 as *mut u8;
        let mut start = ptr as usize;
        start = (start >> 9) << 9;
        let end = start + len;

        #[cfg(feature = "stat_print_flushes")]
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
                }
                #[cfg(all(feature = "use_clwb", not(feature = "use_clflushopt")))]
                {
                    llvm_asm!("clwb ($0)" :: "r"(start as *const u8));
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
#[inline(always)]
pub fn clflush_obj<T: ?Sized>(obj: &T, fence: bool) {
    #[cfg(not(feature = "no_persist"))]
    {
        let len = std::mem::size_of_val(obj);
        clflush(obj, len, fence)
    }
}

/// Store fence
#[inline(always)]
pub fn sfence() {
    #[cfg(any(feature = "use_clwb", feature = "use_clflushopt"))] unsafe {
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
