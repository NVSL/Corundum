use crate::utils::Ring;
use crate::alloc::MemPool;
use crate::result::Result;
use crate::stm::{Chaperon, Journal};
use crate::*;
use std::alloc::{alloc, dealloc, Layout};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Mutex;
use std::thread::ThreadId;

#[cfg(feature = "verbose")]
use term_painter::Color::*;

#[cfg(feature = "verbose")]
use term_painter::ToStyle;

/// A pass-through allocator for volatile memory
pub struct Heap {}

static mut JOURNALS: Option<HashMap<ThreadId, (&Journal<Heap>, i32)>> = None;
static mut CHAPERONS: Option<HashMap<ThreadId, Chaperon>> = None;
static mut MUTEX: Option<Mutex<bool>> = None;

lazy_static! {
    static ref LOGS: Mutex<Ring<(u64, u64), 8>> = Mutex::new(Ring::new());
}

unsafe impl MemPool for Heap {
    #[inline]
    fn rng() -> Range<u64> {
        0..u64::MAX
    }

    #[cfg(not(feature = "verbose"))]
    unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize) {
        let x = alloc(Layout::from_size_align_unchecked(size, 1));
        let off = x as u64;
        (x, off, size)
    }

    #[cfg(not(feature = "verbose"))]
    unsafe fn pre_dealloc(ptr: *mut u8, size: usize) {
        dealloc(ptr, Layout::from_size_align_unchecked(size, 1))
    }

    unsafe fn pre_realloc(ptr: *mut *mut u8, old: usize, new: usize) -> bool {
        dealloc(*ptr, Layout::from_size_align_unchecked(old, 1));
        *ptr = alloc(Layout::from_size_align_unchecked(new, 1));
        true
    }

    #[cfg(feature = "verbose")]
    unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize) {
        let r = alloc(Layout::from_size_align_unchecked(size, 1));
        let addr = r as u64;
        let len = size as u64;
        println!(
            "{}",
            Green.paint(format!(
                "                     PRE: {:<6}  ({:>4}..{:<4}) = {:<4}  POST = {:<6}",
                0,
                addr,
                addr + len - 1,
                len,
                0
            ))
        );
        (r, addr, size)
    }

    #[cfg(feature = "verbose")]
    unsafe fn pre_dealloc(ptr: *mut u8, size: usize) {
        let start = ptr as u64;
        let end = start + size as u64;
        println!(
            "{}",
            Red.paint(format!(
                "          DEALLOC    PRE: {:<6}  ({:>4}..{:<4}) = {:<4}  POST = {:<6}",
                0,
                start,
                end,
                end - start + 1,
                0
            ))
        );
        dealloc(ptr, Layout::from_size_align_unchecked(size, 1));
    }

    fn allocated(off: u64, _len: usize) -> bool {
        if off >= Self::end() {
            false
        } else {
            Self::contains(off + Self::start())
        }
    }

    unsafe fn log64(obj: *const u64, val: u64) {
        LOGS.lock().unwrap().push((obj as *const u8 as u64, val))
    }

    unsafe fn drop_on_failure(_off: u64, _len: usize) {}

    unsafe fn perform() {
        LOGS.lock().unwrap().foreach(|(off, data)| {
            union U<'a> {
                off: u64,
                raw: &'a mut u64,
            }
            *U {off}.raw = data;
        })
    }

    fn open_no_root(_path: &str, _flags: u32) -> Result<Self> {
        Ok(Self {})
    }

    unsafe fn format(_path: &str) -> Result<()> {
        Ok(())
    }

    fn size() -> usize {
        usize::MAX - 1
    }

    fn available() -> usize {
        usize::MAX - 1
    }

    unsafe fn recover() {}

    unsafe fn guarded<T, F: FnOnce() -> T>(f: F) -> T {
        if let Some(mutex) = &mut MUTEX {
            let _guard = mutex.lock();
            f()
        } else {
            MUTEX = Some(Mutex::new(false));
            Self::guarded(f)
        }
    }

    unsafe fn new_journal(tid: ThreadId) {
        if JOURNALS.is_none() {
            JOURNALS = Some(HashMap::new());
        }
        let (journal, offset, size) = Self::atomic_new(Journal::<Self>::new());
        Self::drop_on_failure(offset, size);
        Self::perform();
        JOURNALS
            .as_mut()
            .unwrap()
            .insert(tid, (journal, 0));
    }

    unsafe fn drop_journal(journal: &mut Journal<Self>) {
        let tid = std::thread::current().id();
        JOURNALS.as_mut().unwrap().remove(&tid);
        Self::free_nolog(journal);
        Self::perform();
    }

    unsafe fn journals() -> &'static mut HashMap<ThreadId, (&'static Journal<Self>, i32)> {
        if JOURNALS.is_none() {
            JOURNALS = Some(HashMap::new());
        }
        JOURNALS.as_mut().unwrap()
    }

    unsafe fn close() -> Result<()> {
        Ok(())
    }
}
