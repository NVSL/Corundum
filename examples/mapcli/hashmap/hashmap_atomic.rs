use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = Allocator;

pub struct HashmapAtomic {}

impl<K, V> Map<K, V> for HashmapAtomic {}

impl RootObj<P> for HashmapAtomic {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
