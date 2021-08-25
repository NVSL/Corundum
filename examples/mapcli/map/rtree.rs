use crate::map::Map;
use corundum::default::*;

use std::collections::BTreeMap;

type P = Allocator;

pub struct RTree<K, V> {
    btree: BTreeMap<K, V>,
}

impl<K, V> RTree<K, V> {
    fn self_mut(&self) -> &mut Self {
        unsafe { &mut *(self as *const Self as *mut Self) }
    }
}

impl<K, V: Copy> Map<K, V> for RTree<K, V>
where
    K: std::cmp::Ord,
{
    fn clear(&self) {
        self.self_mut().btree.clear();
    }
    fn insert(&self, key: K, val: V) {
        self.self_mut().btree.insert(key, val);
    }
    fn remove(&self, key: K) {
        self.self_mut().btree.remove(&key);
    }
    fn is_empty(&self) -> bool {
        self.btree.is_empty()
    }
    fn foreach<F: Copy + Fn(&K, &V) -> bool>(&self, f: F) -> bool {
        for (k, v) in &self.btree {
            f(k, v);
        }
        true
    }
    fn lookup(&self, key: K) -> bool {
        self.btree.get(&key).is_some()
    }
}

impl<K: std::cmp::Ord, V> Default for RTree<K, V> {
    fn default() -> Self {
        Self {
            btree: BTreeMap::new(),
        }
    }
}
