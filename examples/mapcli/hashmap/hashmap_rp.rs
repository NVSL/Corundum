use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct HashmapRp {}

impl<K, V> Map<K, V> for HashmapRp {}

impl RootObj<P> for HashmapRp {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
