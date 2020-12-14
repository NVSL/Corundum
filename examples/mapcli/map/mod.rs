mod btree;
mod ctree;
mod pbtree;
mod rbtree;
mod rtree;
mod ubtree;
mod vbtree;

pub use btree::*;
pub use ctree::*;
pub use pbtree::*;
pub use rbtree::*;
pub use rtree::*;
pub use ubtree::*;
pub use vbtree::*;

use crndm::default::*;

pub type P = BuddyAlloc;

pub trait Map<K, V> {
    fn clear(&self) {}
    fn insert(&self, _key: K, _val: V) {}
    fn remove(&self, _key: K) {}
    fn is_empty(&self) -> bool {
        true
    }
    fn foreach<F: Copy + Fn(&K, &V) -> bool>(&self, _f: F) -> bool {
        unimplemented!()
    }
    fn lookup(&self, _key: K) -> Option<&V> {
        None
    }
}
