use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct HashmapAtomic {}

impl<K, V> Map<K, V> for HashmapAtomic {}

impl RootObj<P> for HashmapAtomic {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
