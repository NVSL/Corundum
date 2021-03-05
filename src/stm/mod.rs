//! Software transactional memory APIs

mod chaperon;
mod journal;
mod log;
pub mod pspd;
pub mod vspd;

use crate::alloc::MemPool;
use crate::result::Result;
use crate::{TxInSafe,TxOutSafe};
use std::panic::UnwindSafe;

pub use chaperon::*;
pub use journal::*;
pub use log::*;

/// Atomically executes commands
/// 
/// See [`MemPool::transaction()`](../alloc/trait.MemPool.html#method.transaction)
/// for more details.
pub fn transaction<T, F: FnOnce(&Journal<A>) -> T, A: MemPool>(body: F) -> Result<T>
where
    F: TxInSafe + UnwindSafe,
    T: TxOutSafe,
{
    A::transaction(body)
}
