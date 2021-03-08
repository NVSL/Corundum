use std::ops::Deref;
use std::panic::{UnwindSafe, RefUnwindSafe};
use std::cell::Cell;
use std::mem::MaybeUninit;

/// A memory cell which is initialized on the first access
pub struct LazyCell<T, F = fn() -> T> {
    cell: Cell<Option<T>>,
    init: Cell<Option<F>>,
}

unsafe impl<T, F: Send> Sync for LazyCell<T, F> where MaybeUninit<T>: Sync {}
impl<T, F: UnwindSafe> RefUnwindSafe for LazyCell<T, F> where Cell<T>: RefUnwindSafe {}
impl<T, F: UnwindSafe> UnwindSafe for LazyCell<T, F> where Cell<T>: UnwindSafe {}

impl<T, F> LazyCell<T, F> {
    /// Creates a new lazy value with the given initializing function.
    pub const fn new(f: F) -> LazyCell<T, F> {
        LazyCell { cell: Cell::new(None), init: Cell::new(Some(f)) }
    }
}

impl<T, F: FnOnce() -> T> LazyCell<T, F> {
    #[inline]
    #[track_caller]
    pub fn force(this: &LazyCell<T, F>) -> &T {
        let cell = unsafe { &mut *this.cell.as_ptr() };
        if cell.is_none() {
            match this.init.take() {
                Some(f) => *cell = Some(f()),
                None => panic!("Lazy instance has previously been poisoned"),
            }
        }
        cell.as_ref().unwrap()
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyCell<T, F> {
    type Target = T;
    
    #[track_caller]
    fn deref(&self) -> &T {
        Self::force(self)
    }
}

impl<T: Default> Default for LazyCell<T> {
    /// Creates a new lazy value using `Default` as the initializing function.
    fn default() -> LazyCell<T> {
        LazyCell::new(T::default)
    }
}
