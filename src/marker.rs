//! Corundum Markers
//! 
use crate::stm::Journal;
use crate::alloc::MemPool;
use std::task::Poll;
use std::task::Context;
use std::pin::Pin;
use std::ops::{Deref, DerefMut};
use std::future::Future;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::cell::UnsafeCell;
use std::fmt;

/// It marks the implementing type to be free of pointers to the volatile heap,
/// and persistence safe.
///
/// Also, every type that allows interior mutability is not safe in persistence
/// terms, because there might be no log of the value. Atomic types are
/// persistence safe, even though they provide interior mutability.
/// 
/// # Limitation
/// 
/// Function pointers are not completely prevented. Due to Rust's limitation on
/// declaring generic pointers to functions without exact number of arguments,
/// we manually limit all pointers to functions with up to 32 arguments. Function
/// pointers with a number of arguments beyond 32 are inevitably allowed.
/// 
#[rustc_on_unimplemented(
    message = "`{Self}` is not safe to be stored in persistent memory",
    label = "`{Self}` is not safe to be stored in persistent memory"
)]
pub unsafe auto trait PSafe {}

impl<T: ?Sized> !PSafe for *const T {}
impl<T: ?Sized> !PSafe for *mut T {}
impl<T> !PSafe for &T {}
impl<T> !PSafe for &mut T {}
impl !PSafe for std::fs::File {}

impl<R> !PSafe for fn()->R {}

macro_rules! not_safe {
    ($($a:ident),*) => {
        impl<$($a),* , R> !PSafe for fn($($a),*)->R {}
    };
}

not_safe!(A1);
not_safe!(A1,A2);
not_safe!(A1,A2,A3);
not_safe!(A1,A2,A3,A4);
not_safe!(A1,A2,A3,A4,A5);
not_safe!(A1,A2,A3,A4,A5,A6);
not_safe!(A1,A2,A3,A4,A5,A6,A7);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27,A28);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27,A28,A29);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27,A28,A29,A30);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27,A28,A29,A30,A31);
not_safe!(A1,A2,A3,A4,A5,A6,A7,A8,A9,A10,A11,A12,A13,A14,A15,A16,A17,A18,A19,A20,A21,A22,A23,A24,A25,A26,A27,A28,A29,A30,A31,A32);

/// `UnsafeCell` is marked as PSafe because it exposes interior mutability
/// without taking a log, which is unsafe from persistence perspective.
impl<T: ?Sized> !PSafe for UnsafeCell<T> {}

/// It marks the implementing type to be safe crossing transaction boundaries
///
/// Types that implement this trait may go in/out of a transaction. This
/// guarantees no cross-pool referencing.
#[rustc_on_unimplemented(
    message = "`{Self}` cannot be sent out of a transaction safely",
    label = "`{Self}` cannot be sent out of a transaction safely"
)]
pub unsafe auto trait TxOutSafe {}

impl<T: ?Sized> !TxOutSafe for *const T {}
impl<T: ?Sized> !TxOutSafe for *mut T {}
impl<T: ?Sized> !TxOutSafe for &mut T {}
impl<T: ?Sized> !TxOutSafe for UnsafeCell<T> {}

unsafe impl TxOutSafe for String {}
unsafe impl<T> TxOutSafe for std::thread::JoinHandle<T> {}
unsafe impl<T> TxOutSafe for Vec<std::thread::JoinHandle<T>> {}

/// It is equal to UnwindSafe, but is used to ensure doubly that mutable
/// references cannot go inside a transaction.
///
/// # Safety
///
/// The user can safely specify a type as `UnwindSafe`, but `TxInSafe` is
/// unsafe to implement. This warns the programmer that the non-existence
/// of orphans is not guaranteed anymore.
#[rustc_on_unimplemented(
    message = "`{Self}` cannot be sent to a transaction safely",
    label = "`{Self}` cannot be sent to a transaction safely"
)]
pub unsafe auto trait TxInSafe {}

/// The implementing type can be asserted [`TxInSafe`] albeit being `!TxInSafe`
/// by using [`AssertTxInSafe`](./struct.AssertTxInSafe.html).
/// 
/// [`TxInSafe`]: ./trait.TxInSafe.html
#[rustc_on_unimplemented(
    message = "`{Self}` cannot be asserted as `TxInSafe`",
    label = "`{Self}` cannot be asserted as `TxInSafe`"
)]
pub unsafe auto trait LooseTxInUnsafe {}

/// Any type is okay to be transferred to a transaction
unsafe impl LooseTxInUnsafe for dyn std::any::Any {}
unsafe impl<'a, T> LooseTxInUnsafe for &'a mut T {}
unsafe impl<T> LooseTxInUnsafe for *const T {}
unsafe impl<T> LooseTxInUnsafe for *mut T {}

/// A simple wrapper around a type to assert that it is safe to go in a
/// transaction.
///
/// When using [`transaction`] it may be the case that some of the closed over
/// variables are not [`TxInSafe`] safe. For example if `&mut T` is captured the
/// compiler will generate a warning indicating that it is not [`TxInSafe`]. It
/// may not be the case, however, that this is actually a problem due to the
/// specific usage of [`transaction`] if transaction inward safety is
/// specifically taken into account. This wrapper struct is useful for a quick
/// and lightweight annotation that a variable is indeed [`TxInSafe`] at the
/// programmer's responsibilities. The `Journal` object cannot be wrapped by 
/// `AssertTxInSafe` to make sure no inter-pool pointer can be made.
///
/// # Examples
/// 
/// You may wrap individual captures, as shown below. This ensures that if a new
/// capture is added which is not [`TxInSafe`], you will get a compilation error
/// at that time, which will allow you to consider whether that new capture in
/// fact represent a bug or not. 
///
/// ```
/// use corundum::alloc::heap::*;
/// use corundum::AssertTxInSafe;
///
/// let mut variable = 4;
/// let other_capture = 3;
///
/// let result = {
///     let mut wrapper = AssertTxInSafe(&mut variable);
///     Heap::transaction(move |_| {
///         **wrapper += other_capture;
///     })
/// };
/// 
/// assert_eq!(variable, 7);
/// // ...
/// ```
/// 
/// [`transaction`]: ./stm/fn.transaction.html
/// [`TxInSafe`]: ./trait.TxInSafe.html
pub struct AssertTxInSafe<T>(pub T);

impl<T: ?Sized> !TxInSafe for *mut T {}
impl<T: ?Sized> !TxInSafe for &mut T {}
impl<T: ?Sized> !TxInSafe for UnsafeCell<T> {}
impl<T: LooseTxInUnsafe> UnwindSafe for AssertTxInSafe<T> {}
impl<T: LooseTxInUnsafe> RefUnwindSafe for AssertTxInSafe<T> {}
unsafe impl<T: LooseTxInUnsafe> TxInSafe for AssertTxInSafe<T> {}

impl<T: LooseTxInUnsafe> Deref for AssertTxInSafe<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: LooseTxInUnsafe> DerefMut for AssertTxInSafe<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<R, P: MemPool, F> FnOnce<(&'static Journal<P>,)> for AssertTxInSafe<F>
where
    R: TxOutSafe,
    F: FnOnce(&'static Journal<P>) -> R
{
    type Output = R;

    #[inline]
    extern "rust-call" fn call_once(self, args: (&'static Journal<P>,)) -> R  
    {
        (self.0)(args.0)
    }
}

impl<T: fmt::Debug + LooseTxInUnsafe> fmt::Debug for AssertTxInSafe<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AssertTxInSafe").field(&self.0).finish()
    }
}

impl<F: Future + LooseTxInUnsafe> Future for AssertTxInSafe<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pinned_field = unsafe { Pin::map_unchecked_mut(self, |x| &mut x.0) };
        F::poll(pinned_field, cx)
    }
}

/// Safe to be stored in volatile memory useful in `VCell` type to prevent
/// storing persistent pointers in [`VCell`](./cell/struct.VCell.html)
#[rustc_on_unimplemented(
    message = "`{Self}` is not safe to be stored in volatile memory",
    label = "`{Self}` is not safe to be stored in volatile memory"
)]
pub unsafe auto trait VSafe {}

unsafe impl<T: ?Sized> VSafe for *const T {}
unsafe impl<T: ?Sized> VSafe for *mut T {}
unsafe impl<T: ?Sized> VSafe for &T {}
unsafe impl<T: ?Sized> VSafe for &mut T {}

/// Safe to be sent to another thread
/// 
/// This marker is used to allow [`Parc`] to be sent to another thread only if
/// it is wrapped in a [`VWeak`]. The [`Parc`] is not [`Send`] to prevent
/// escaping a newly allocated instance of it from a transaction.
/// 
/// [`Parc`]: ../sync/struct.Parc.html
/// [`Send`]: ../trait.Send.html
/// [`VWeak`]: ../sync/struct.VWeak.html
#[rustc_on_unimplemented(
    message = "`{Self}` cannot be sent to a another thread safely",
    label = "`{Self}` cannot be sent to a another thread safely"
)]
pub unsafe auto trait PSend {}



#[derive(Default)]
#[allow(non_camel_case_types)]
pub struct c_void {}

impl Copy for c_void {}
impl Clone for c_void { 
    fn clone(&self) -> Self { Self { } }
}

impl<P: MemPool> crate::clone::PClone<P> for c_void {
    fn pclone(&self, _: &Journal<P>) -> Self { Self { } }
}