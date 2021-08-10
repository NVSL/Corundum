use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = Allocator;

pub struct RbTree {}

impl<K, V> Map<K, V> for RbTree {}

impl RootObj<P> for RbTree {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
