//! Persistent Scratchpad Memory
use crate::cell::LazyCell;
use crate::alloc::MemPool;
use crate::ptr::Ptr;
use crate::{utils, ll};
use std::{mem, ptr};

static SCRATCHPAD_SIZE: LazyCell<usize> = LazyCell::new(|| {
    utils::nearest_pow2(std::env::var("SPD_SIZE")
        .unwrap_or("1024".to_string())
        .parse::<u64>()
        .expect("RECOVERY_INFO should be an unsigned integer")) as usize
});

struct Page<A: MemPool> {
    len: usize,
    cap: usize,
    next: Ptr<Page<A>, A>
}

impl<A: MemPool> Page<A> {
    unsafe fn write<T: ?Sized>(&mut self, val: &T, org_off: u64) -> *mut T {
        let size = mem::size_of_val(val);

        // Data Layout:
        //   * org_off                           (u64)
        //   * relative distance from next item  (u64)
        //   * data                              (T)
        let dist = 8 + 8 + size;
        if self.len + dist > self.cap {
            if let Some(next) = self.next.as_option() {
                next.write(val, org_off)
            } else {
                let cap = *SCRATCHPAD_SIZE;
                let cap = utils::nearest_pow2(usize::max(cap, dist) as u64) as usize;
                // FIXME: Memory leak
                let (p, off, _, z) = A::pre_alloc(cap);
                let pg = utils::read::<Page<A>>(p);
                pg.cap = cap - mem::size_of::<Page<A>>();
                pg.len = 0;
                pg.next = self.next;
                A::log64(A::off_unchecked(self.next.off_mut()), off, z);
                A::perform(z);
                pg.write(val, org_off)
            }
        } else {
            let p = self as *mut Self as *mut u8;
            let p = p.add(mem::size_of::<Self>());
            
            // First 8 bytes is org_off
            let p = p.add(self.len);
            *utils::read::<u64>(p) = org_off;
            
            // Second 8 bytes is the relative distance
            let p = p.add(8);
            *utils::read::<usize>(p) = dist;
            
            // The last bytes contain data
            let p = p.add(8);
            ptr::copy_nonoverlapping(val as *const _ as *const u8, p, size);
    
            self.len += dist;
            utils::read(p)
        }
    }

    unsafe fn apply(&mut self) {
        let off = A::off_unchecked(self) + mem::size_of::<Page<A>>() as u64;
        let mut cur = 0;
        while cur < self.len as u64 {
            let p = utils::read_addr::<u8>(off + cur + A::start()) as *mut u8;
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

        if let Some(next) = self.next.as_option() {
            next.apply();
        }
    }

    unsafe fn release(&mut self, org_off: u64) {
        let next_off = A::off_unchecked(self.next.off_mut());
        if let Some(next) = self.next.as_option() {
            next.release(next_off);
        }

        if self.cap != 0 {
            let z = A::pre_dealloc(self as *mut _ as *mut u8, mem::size_of::<Page<A>>() + self.cap);
            A::log64(org_off, u64::MAX, z);
            A::perform(z);
        }
    }
}

pub struct Scratchpad<A: MemPool> {
    pages: Ptr<Page<A>, A>,
    committed: bool
}

impl<A: MemPool> Scratchpad<A> {
    pub(crate) fn new() -> Self {
        unsafe {
            let cap = *SCRATCHPAD_SIZE - mem::size_of::<Page<A>>();
            // FIXME: Memory leak
            let (p, _off, _) = A::alloc(mem::size_of::<Page<A>>() + cap);
            let pg = utils::read::<Page<A>>(p);
            pg.cap = cap;
            pg.len = 0;
            pg.next = Ptr::dangling();
            Self {
                pages: Ptr::from_raw(pg),
                committed: false
            }
        }
    }

    #[inline]
    pub(crate) unsafe fn write<T: ?Sized>(&mut self, val: &T, off: u64) -> *mut T {
        self.pages.write(val, off)
    }

    #[inline]
    pub(crate) unsafe fn recover(&mut self) {
        if self.committed {
            self.commit();
        }
    }

    #[inline]
    pub(crate) unsafe fn commit(&mut self) {
        ll::sfence();
        self.committed = true;
        ll::persist_obj(&self.committed, false);
        self.pages.apply();
    }

    #[inline]
    pub(crate) unsafe fn rollback(&mut self) {
        // Do nothing
    }

    #[inline]
    pub(crate) unsafe fn clear(&mut self) {
        #[cfg(not(feature = "pin_journals"))] {
            let org_off = A::off_unchecked(self.pages.off_mut());
            self.pages.release(org_off);
        }
        #[cfg(feature = "pin_journals")] {
            let next_off = A::off_unchecked(self.pages.next.off_mut());
            if let Some(next) = self.pages.next.as_option() {
                next.release(next_off);
            }
            self.pages.len = 0;
        }
    }
}
