use crate::map::Map;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::RootObj;

type P = BuddyAlloc;

pub struct CTree {}

impl<K, V> Map<K, V> for CTree {}

impl RootObj<P> for CTree {
    fn init(_: &Journal<P>) -> Self {
        todo!()
    }
}