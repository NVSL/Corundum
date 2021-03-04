use std::sync::Arc;
use crate::cell::RootCell;
use crate::utils::*;
use crate::result::Result;
use crate::stm::Chaperon;
use crate::*;
use std::alloc::{alloc, dealloc, Layout};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Mutex;
use std::thread::ThreadId;
use crate::cell::LazyCell;

pub use crate::alloc::*;

/// A pass-through allocator for volatile memory
pub struct Heap {}

static mut JOURNALS: Option<HashMap<ThreadId, (u64, i32)>> = None;
static mut CHAPERONS: Option<HashMap<ThreadId, Chaperon>> = None;
static mut MUTEX: Option<Mutex<bool>> = None;
static mut LOGS: LazyCell<Mutex<Ring<(u64, u64), 8>>> = 
    LazyCell::new(|| Mutex::new(Ring::new()));

unsafe impl MemPool for Heap {
    fn name() -> &'static str {
        "heap"
    }

    #[inline]
    fn rng() -> Range<u64> {
        0..u64::MAX
    }

    #[cfg(not(feature = "verbose"))]
    unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize, usize) {
        Self::discard(0);
        let x = alloc(Layout::from_size_align_unchecked(size, 1));
        let off = x as u64;
        (x, off, size, 0)
    }

    #[cfg(not(feature = "verbose"))]
    unsafe fn pre_dealloc(ptr: *mut u8, size: usize) -> usize {
        Self::discard(0);
        dealloc(ptr, Layout::from_size_align_unchecked(size, 1));
        0
    }

    #[cfg(feature = "verbose")]
    unsafe fn pre_alloc(size: usize) -> (*mut u8, u64, usize, usize) {
        Self::discard(0);
        let r = alloc(Layout::from_size_align_unchecked(size, 1));
        let addr = r as u64;
        let len = size as u64;
        log!(Self, Green, "", "PRE: {:<6}  ({:>4}..{:<4}) = {:<4}  POST = {:<6}",
            0, addr, addr + len - 1, len, 0);
        (r, addr, size, 0)
    }

    #[cfg(feature = "verbose")]
    unsafe fn pre_dealloc(ptr: *mut u8, size: usize) -> usize {
        Self::discard(0);
        let start = ptr as u64;
        let end = start + size as u64;
        log!(Self, Red, "DEALLOC", "PRE: {:<6}  ({:>4}..{:<4}) = {:<4}  POST = {:<6}",
            0, start, end, end - start + 1, 0);
        dealloc(ptr, Layout::from_size_align_unchecked(size, 1));
        0
    }

    fn allocated(off: u64, _len: usize) -> bool {
        if off >= Self::end() {
            false
        } else {
            Self::contains(off + Self::start())
        }
    }

    unsafe fn log64(obj: u64, val: u64, _: usize) {
        let mut logs = match LOGS.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner()
        };
        logs.push((obj, val))
    }

    unsafe fn drop_on_failure(_off: u64, _len: usize, _: usize) {}

    unsafe fn perform(_: usize) {
        let mut logs = match LOGS.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner()
        };
        logs.foreach(|(off, data)| {
            *utils::read_addr(off) = data;
        });
        logs.clear();
    }

    unsafe fn discard(_: usize) {
        let mut logs = match LOGS.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner()
        };
        logs.clear()
    }

    fn open_no_root(_path: &str, _flags: u32) -> Result<Self> {
        Ok(Self {})
    }

    #[track_caller]
    fn open<'a, U: 'a + PSafe + RootObj<Self>>(
        path: &str,
        flags: u32,
    ) -> Result<RootCell<'a, U, Self>> {
        let slf = Self::open_no_root(path, flags)?;
        if std::mem::size_of::<U>() == 0 {
            Err("root type cannot be a ZST".to_string())
        } else {
            unsafe {
                let root_off = Self::transaction(move |j| {
                    let ptr = Self::new(U::init(j), j);
                    Self::off_unchecked(ptr)
                })
                .unwrap();
                let ptr = Self::get_unchecked(root_off);
                Ok(RootCell::new(ptr, Arc::new(slf)))
            }
        }
    }

    fn is_open() -> bool {
        true
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

    unsafe fn drop_journal(journal: &mut Journal) {
        let tid = std::thread::current().id();
        JOURNALS.as_mut().unwrap().remove(&tid);
        Self::free_nolog(journal);
    }

    unsafe fn journals<T, F: Fn(&mut HashMap<ThreadId, (u64, i32)>)->T>(f: F)->T{
        if JOURNALS.is_none() {
            JOURNALS = Some(HashMap::new());
        }
        f(JOURNALS.as_mut().unwrap())
    }

    unsafe fn journals_head() -> &'static u64 {
        static mut HEAD: u64 = u64::MAX;
        &HEAD
    }

    unsafe fn close() -> Result<()> {
        Ok(())
    }
}

/// Compact form of [`Pbox`](../../boxed/struct.Pbox.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type Pbox<T> = crate::boxed::Pbox<T, Heap>;

/// Compact form of [`Prc`](../../prc/struct.Prc.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type Prc<T> = crate::prc::Prc<T, Heap>;

/// Compact form of [`Parc`](../../sync/struct.Parc.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type Parc<T> = crate::sync::Parc<T, Heap>;

/// Compact form of [`PMutex`](../../sync/struct.PMutex.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type PMutex<T> = crate::sync::PMutex<T, Heap>;

/// Compact form of [`PCell`](../../cell/struct.PCell.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type PCell<T> = crate::cell::PCell<T, Heap>;

/// Compact form of [`LogNonNull`](../../ptr/struct.LogNonNull.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type PNonNull<T> = crate::ptr::LogNonNull<T, Heap>;

/// Compact form of [`PRefCell`](../../cell/struct.PRefCell.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type PRefCell<T> = crate::cell::PRefCell<T, Heap>;

/// Compact form of [`Ref`](../../cell/struct.Ref.html)
/// `<'b, T, `[`Heap`](./struct.Heap.html)`>`.
pub type PRef<'b, T> = crate::cell::Ref<'b, T, Heap>;

/// Compact form of [`RefMut`](../../cell/struct.Mut.html)
/// `<'b, T, `[`Heap`](./struct.Heap.html)`>`.
pub type PRefMut<'b, T> = crate::cell::RefMut<'b, T, Heap>;

/// Compact form of `[VCell](../../cell/struct.VCell.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type VCell<T> = crate::cell::VCell<T, Heap>;

/// Compact form of [`Vec`](../../vec/struct.Vec.html)
/// `<T,`[`Heap`](./struct.Heap.html)`>`.
pub type PVec<T> = crate::vec::Vec<T, Heap>;

/// Compact form of [`String`](../../str/struct.String.html)
/// `<`[`Heap`](./struct.Heap.html)`>`.
pub type PString = crate::str::String<Heap>;

/// Compact form of [`Journal`](../../stm/struct.Journal.html)
/// `<`[`Heap`](./struct.Heap.html)`>`.
pub type Journal = crate::stm::Journal<Heap>;

pub mod prc {
    /// Compact form of [`prc::Weak`](../../../prc/struct.Weak.html)
    /// `<`[`Heap`](./struct.Heap.html)`>`.
    pub type PWeak<T> = crate::prc::Weak<T, super::Heap>;

    /// Compact form of [`prc::VWeak`](../../../prc/struct.VWeak.html)
    /// `<`[`Heap`](../struct.Heap.html)`>`.
    pub type VWeak<T> = crate::prc::VWeak<T, super::Heap>;
}

pub mod parc {
    /// Compact form of [`sync::Weak`](../../../sync/struct.Weak.html)
    /// `<`[`Heap`](../struct.Heap.html)`>`.
    pub type PWeak<T> = crate::sync::Weak<T, super::Heap>;

    /// Compact form of [`sync::VWeak`](../../../sync/struct.VWeak.html)
    /// `<`[`Heap`](../struct.Heap.html)`>`.
    pub type VWeak<T> = crate::sync::VWeak<T, super::Heap>;
}
