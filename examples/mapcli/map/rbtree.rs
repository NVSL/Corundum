use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct RbTree {}

impl<K, V> Map<K, V> for RbTree {}

impl RootObj<P> for RbTree {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
