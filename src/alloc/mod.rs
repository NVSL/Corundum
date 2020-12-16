//! Persistent Memory allocation APIs

mod alg;
mod heap;
mod pool;

pub use alg::buddy::*;
pub use heap::*;
pub use pool::*;

/// Determines how much of the `MemPool` is used for the trait object.
///
/// This is useful for obtaining the size of the referent of the persistent
/// pointers.
pub trait PmemUsage
where
    Self: Sized,
{
    /// Size of the object on Persistent Memory
    /// Assuming that self is not on PM, or considered else were, the size of allocated persistent memory
    /// is the sum of all persistent objects pointed by this object.
    fn size_of() -> usize {
        0
    }
    /// Size of the object on Persistent Memory including `Self`
    /// Assuming that self is also on PM (e.g. the root object), the size of allocated persistent memory
    /// includes the size of all objects pointed by this object and the size `Self`.
    fn size_of_pmem() -> usize {
        Self::size_of() + std::mem::size_of::<Self>()
    }
}
