//! Volatile Scratchpad Memory

use crate::cell::{LazyCell,VCell};
use crate::alloc::MemPool;
use crate::ptr::Ptr;
use crate::{utils, ll};
use std::{mem, ptr, alloc::*};

static SCRATCHPAD_SIZE: LazyCell<usize> = LazyCell::new(|| {
    std::env::var("SPD_SIZE")
        .unwrap_or("1024".to_string())
        .parse::<usize>()
        .expect("RECOVERY_INFO should be an unsigned integer")
});

struct RawPtr(*mut u8);

impl Default for RawPtr {
    fn default() -> Self {
        Self(ptr::null_mut())
    }
}

pub struct Scratchpad<A: MemPool> {
    base: VCell<RawPtr, A>,
    cap: usize,
    len: usize,
    off: u64,
    pm: Ptr<Self, A>
}

impl<A: MemPool> Scratchpad<A> {
    pub(crate) fn new() -> Self {
        let size = *SCRATCHPAD_SIZE;
        Self {
            base: VCell::new(RawPtr(unsafe {
                alloc(Layout::from_size_align_unchecked(size, 2))
            })),
            cap: size,
            len: 0,
            off: u64::MAX,
            pm: Ptr::dangling()
        }
    }

    pub(crate) unsafe fn write<T: ?Sized>(&mut self, val: &T, off: u64) -> *mut T {
        let size = mem::size_of_val(val);

        // Data Layout:
        //   * org_off                           (u64)
        //   * relative distance from next item  (u64)
        //   * data                              (T)
        let len = 8 + 8 + size;
        if self.len + len > self.cap {
            let new_cap = self.cap + *SCRATCHPAD_SIZE;
            self.base = VCell::new(RawPtr(realloc(self.base.0,
                Layout::from_size_align_unchecked(self.cap, 2),
                new_cap)));
            self.cap = new_cap;
        }
        // First 8 bytes is org_off
        let p = self.base.0.add(self.len);
        *utils::read::<u64>(p) = off;

        // Second 8 bytes is the relative distance
        let p = p.add(8);
        *utils::read::<usize>(p) = len;

        // The last bytes contain data
        let p = p.add(8);
        ptr::copy_nonoverlapping(val as *const _ as *const u8, p, size);

        self.len += len;
        utils::read(p)
    }

    unsafe fn apply(&mut self) {
        if self.off != u64::MAX {
            let mut cur = 0;
            while cur < self.len as u64 {
                let p = utils::read_addr::<u8>(cur + self.off + A::start()) as *mut u8;
                let org_off = *utils::read::<u64>(p);
    
                let p = p.add(8);
                let dist = *utils::read::<usize>(p);
    
                let p = p.add(8);
                let len = dist - 16;
                let org = utils::read_addr::<u8>(org_off + A::start());
                ptr::copy_nonoverlapping(p, org, len);
                ll::persist(org, len, false);
    
                cur += dist as u64;
            }
        }
    }

    pub(crate) unsafe fn recover(&mut self) {
        if let Some(spd) = self.pm.try_deref_mut() {
            spd.apply();
        }
    }

    pub(crate) unsafe fn commit(&mut self) {
        let size = mem::size_of::<Self>();
        let (p, off, len, z) = A::pre_alloc(size + self.len);
        let base = off + size as u64;
        let spd = Self {
            base: VCell::new(RawPtr(ptr::null_mut())),
            cap: 0,
            len: self.len,
            off: base,
            pm: Ptr::dangling()
        };
        mem::forget(mem::replace(utils::read(p), spd));
        let spd = utils::read::<Self>(p);
        ptr::copy_nonoverlapping(self.base.0,
            utils::read_addr(base + A::start()),
            self.len);
        A::drop_on_failure(off, len, z);
        ll::persist(spd, size, false);
        A::log64(A::off_unchecked(self.pm.off_mut()), off, z);
        A::perform(z);

        spd.apply();
    }

    pub(crate) unsafe fn rollback(&mut self) {
        // Do nothing
    }

    pub(crate) unsafe fn clear(&mut self) {
        if let Some(spd) = self.pm.try_deref_mut() {
            let z = A::pre_dealloc(spd as *mut _ as *mut u8, mem::size_of_val(spd));
            ptr::drop_in_place(spd);
            A::log64(A::off_unchecked(self.pm.off_mut()), u64::MAX, z);
            A::log64(A::off_unchecked(&self.len), 0, z);
            A::perform(z);
        }
        self.pm = Ptr::dangling();
        self.len = 0;
    }
}

impl<A: MemPool> Drop for Scratchpad<A> {
    fn drop(&mut self) {
        unsafe {
            if !self.base.0.is_null() {
                if self.cap != 0 {
                    dealloc(
                        self.base.0,
                        Layout::from_size_align_unchecked(self.cap, 2)
                    );
                } else if self.len != 0 && self.off != u64::MAX {
                    let z = A::pre_dealloc((self.off as u64 + A::start()) as *mut u8, self.len);
                    A::log64(A::off_unchecked(&self.len), 0, z);
                    A::log64(A::off_unchecked(&self.off), u64::MAX, z);
                    A::perform(z);
                }
            }
        }
    }
}