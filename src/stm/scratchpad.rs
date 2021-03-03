use std::marker::PhantomData;
use crate::alloc::MemPool;

pub struct ScratchPad<A: MemPool, const N: usize> {
    phantom: PhantomData<A>
}