//! The `PClone` trait for types that cannot be 'implicitly copied'

use crate::alloc::*;
use crate::stm::*;
use crate::PSafe;

/// A common trait for the ability to explicitly duplicate an object.
///
/// Differs from [`Copy`] in that [`Copy`] is implicit and extremely
/// inexpensive, while `PClone` is always explicit and may or may not be
/// expensive.  Its difference with [`Clone`] is that it a `&`[`Journal`] to be
/// used for logging in [`Prc`] and [`Parc`]. In order to enforce these
/// characteristics, Rust does not allow you to reimplement [`Copy`], but you
/// may reimplement [`Clone`] and `PClone` and run arbitrary code.
///
/// Since `PClone` is more general than [`Copy`], you can automatically make
/// anything [`Copy`] be `Clone` as well.
///
/// ## Derivable
///
/// This trait can be used with `#[derive]` if all fields are `PClone`. The
/// `derive`d implementation of [`PClone`] calls [`pclone`] on each field. It
/// uses [`default::BuddyAlloc`] by default. It is possible to change the pool
/// type(s) by using `pools()` attribute. 
///
/// [`pclone`]: #method.pclone
/// [`Prc`]: ../prc/struct.Prc.html
/// [`Parc`]: ../sync/struct.Parc.html
///
/// For a generic struct, `#[derive]` implements `PClone` conditionally by
/// adding bound `Clone` on generic parameters.
///
/// ```
/// # use corundum::*;
/// // `derive` implements PClone<BuddyAlloc> for Reading<T> when T is 
/// // PClone<BuddyAlloc>
/// #[derive(PClone)]
/// struct Reading<T> {
///     frequency: T,
/// }
/// ```
/// 
/// ```
/// # use corundum::*;
/// # use corundum::alloc::heap::*;
/// # pool!(p); type P = p::BuddyAlloc;
/// # pool!(q); type Q = q::BuddyAlloc;
/// // `derive` implements PClone<P> and PClone<Q> for Reading<T> when T is 
/// // PClone<P> and PClone<Q> specified by `pools`.
/// #[derive(PClone)]
/// #[pools(P,Q)]
/// struct Reading<T> {
///     frequency: T,
/// }
/// ```
///
/// ## How can I implement `PClone`?
///
/// Types that are [`Copy`] should have a trivial implementation of `PClone`.
/// More formally: if `T: Copy`, `x: T`, and `y: &T`, then `let x = y.pclone();`
/// is equivalent to `let x = *y;`. Manual implementations should be careful to
/// uphold this invariant; however, unsafe code must not rely on it to ensure
/// memory safety. Note that, the implementation should be done for a specific
/// (or generic) pool type.
///
/// [`Copy`]: std::marker::Copy
pub trait PClone<A: MemPool>: Sized {
    fn pclone(&self, journal: &Journal<A>) -> Self;
    /// Performs copy-assignment from `source`.
    ///
    /// `a.clone_from(&b)` is equivalent to `a = b.clone()` in functionality,
    /// but can be overridden to reuse the resources of `a` to avoid unnecessary
    /// allocations.
    fn pclone_from(&mut self, source: &Self, journal: &Journal<A>) {
        *self = source.pclone(journal)
    }
}

impl<T: PSafe + PClone<A>, A: MemPool> PClone<A> for Option<T> {
    fn pclone(&self, journal: &Journal<A>) -> Self {
        if let Some(x) = self {
            Some(x.pclone(journal))
        } else {
            None
        }
    }
}

impl<T: PSafe + PClone<A> + ?Sized, A: MemPool> PClone<A> for &[T] {
    fn pclone(&self, j: &Journal<A>) -> Self {
        let res = unsafe { A::new_slice(self, j) };
        for i in 0..res.len() {
            res[i] = self[i].pclone(j);
        }
        res
    }
}

impl<T: PSafe + PClone<A>, A: MemPool, const N: usize> PClone<A> for [T;N] {
    fn pclone(&self, j: &Journal<A>) -> Self {
        use std::mem::MaybeUninit;
        let mut res = unsafe { MaybeUninit::<Self>::uninit().assume_init() };
        for i in 0..res.len() {
            std::mem::forget(std::mem::replace(&mut res[i],self[i].pclone(j)));
        }
        res
    }
}

use impl_trait_for_tuples::*;

#[impl_for_tuples(32)]
impl<A: MemPool> PClone<A> for Tuple {
    fn pclone(&self, j: &Journal<A>) -> Self {
        for_tuples!( ( #( Tuple.pclone(j) ),* ) )
    }
}

/// Implementations of `PClone` for primitive types.
mod impls {

    use super::PClone;
    use crate::alloc::MemPool;
    use crate::stm::Journal;

    macro_rules! impl_clone {
        ($($t:ty)*) => {
            $(
                impl<A: MemPool> PClone<A> for $t {
                    #[inline]
                    fn pclone(&self, _j: &Journal<A>) -> Self {
                        *self
                    }
                }
            )*
        }
    }

    impl_clone! {
        usize u8 u16 u32 u64 u128
        isize i8 i16 i32 i64 i128
        f32 f64
        bool char
    }
}
