use crate::map::Map;
use crndm::default::*;
use crndm::stm::Journal;
use crndm::RootObj;

type P = BuddyAlloc;

pub struct CTree {}

impl<K, V> Map<K, V> for CTree {}

impl RootObj<P> for CTree {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}