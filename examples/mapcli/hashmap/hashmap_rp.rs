use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = Allocator;

pub struct HashmapRp {}

impl<K, V> Map<K, V> for HashmapRp {}

impl RootObj<P> for HashmapRp {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
