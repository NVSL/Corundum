use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = BuddyAlloc;

pub struct Skiplist {}

impl<K, V> Map<K, V> for Skiplist {}

impl RootObj<P> for Skiplist {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}
