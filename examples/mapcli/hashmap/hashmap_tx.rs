use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct HashmapTx {}

impl<K, V> Map<K, V> for HashmapTx {}

impl RootObj<P> for HashmapTx {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
