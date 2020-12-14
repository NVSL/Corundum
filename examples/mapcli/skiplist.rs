use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct Skiplist {}

impl<K, V> Map<K, V> for Skiplist {}

impl RootObj<P> for Skiplist {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
