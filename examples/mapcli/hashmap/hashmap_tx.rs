use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = Allocator;

pub struct HashmapTx {}

impl<K, V> Map<K, V> for HashmapTx {}

impl RootObj<P> for HashmapTx {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
