//! Low-level utils
#![allow(unused)]

use crate::alloc::MemPool;
use std::arch::asm;

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
pub fn persist_with_log<T: ?Sized, A: MemPool>(ptr: *const T, len: usize, fence: bool) {
    unsafe {
        crate::log!(A, BrightCyan, "PERSIST", "             ({:>6x}:{:<6x}) = {:<6}",
            A::off_unchecked(ptr),
            A::off_unchecked(ptr) + (len as u64 - 1), len   
        );
    }
    persist(ptr, len, fence)
}

/// Synchronize caches and memories and acts like a write barrier
#[inline(always)]
pub fn persist<T: ?Sized>(ptr: *const T, len: usize, fence: bool) {
    #[cfg(feature = "stat_perf")]
    let _perf = crate::stat::Measure::<crate::default::Allocator>::Sync(std::time::Instant::now());

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

#[inline(always)]
pub fn persist_obj_with_log<T: ?Sized, A: MemPool>(obj: &T, fence: bool) {
    #[cfg(not(feature = "no_persist"))]
    {
        persist_with_log::<T, A>(obj, std::mem::size_of_val(obj), fence);
    }
}

/// Synchronize caches and memories and acts like a write barrier
#[inline(always)]
pub fn persist_obj<T: ?Sized>(obj: &T, fence: bool) {
    #[cfg(feature = "stat_perf")]
    let _perf = crate::stat::Measure::<crate::default::Allocator>::Sync(std::time::Instant::now());

    #[cfg(not(feature = "no_persist"))]
    {
        persist(obj, std::mem::size_of_val(obj), fence);
    }
}

/// Flushes cache line back to memory
#[inline(always)]
pub fn clflush<T: ?Sized>(ptr: *const T, len: usize, fence: bool) {
    #[cfg(not(feature = "no_persist"))]
    {
        let ptr = ptr as *const u8 as *mut u8;
        let mut start = ptr as usize;
        start = (start >> 9) << 9;
        let end = start + len;

        #[cfg(feature = "stat_print_flushes")]
        println!("flush {:x} ({})", start, len);

        while start < end {
            unsafe {
                #[cfg(not(any(feature = "use_clflushopt", feature = "use_clwb")))]
                {
                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    asm!("clflush [{}]", in(reg) (start as *const u8), options(nostack));
                    
                    #[cfg(target_arch = "aarch64")]
                    asm!("dc cvau, {}", in(reg) (start as *const u8))
                }
                #[cfg(all(feature = "use_clflushopt", not(feature = "use_clwb")))]
                {
                    asm!("clflushopt [{}]", in(reg) (start as *const u8), options(nostack));
                }
                #[cfg(all(feature = "use_clwb", not(feature = "use_clflushopt")))]
                {
                    asm!("clwb [{}]", in(reg) (start as *const u8), options(nostack));
                }
                #[cfg(all(feature = "use_clwb", feature = "use_clflushopt"))]
                {
                    compile_error!("Please Select only one from clflushopt and clwb")
                }
            }
            start += 64;
        }
    }
    if (fence) {
        sfence();
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
        std::intrinsics::atomic_fence()
    }
}
