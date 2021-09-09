#![allow(dead_code)]

use std::fmt::Display;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::*;
use crate::alloc::*;
use crate::vec::Vec as PVec;
use crate::cell::PRefCell;
use crate::stm::Journal;
use crate::clone::PClone;

const BUCKETS_MAX: usize = 16;

type Bucket<K, P: MemPool> = PVec<PRefCell<(K, usize),P>,P>;

pub struct HashMap<K: PSafe, V: PSafe, P: MemPool> {
    buckets: PVec<PRefCell<Bucket<K,P>,P>,P>,
    values: PVec<PRefCell<V,P>,P>,
}

impl<K: PartialEq + Hash + PSafe, V: PSafe, P: MemPool> RootObj<P> for HashMap<K, V, P> {
    fn init(j: &Journal<P>) -> Self {
        Self::new(j)
    }
}

impl<K: PSafe, V: PSafe, P: MemPool> HashMap<K, V, P> {
    pub fn foreach<F: FnMut(&K, &V) -> ()>(&self, mut f: F) {
        for i in 0..BUCKETS_MAX {
            for e in &*self.buckets[i].borrow() {
                let e = e.borrow();
                f(&e.0, &self.values[e.1].borrow());
            }
        }
    }
}

impl<K: PSafe, V: PSafe, P: MemPool> HashMap<K, V, P>
where
    K: PartialEq + Hash
{
    pub fn new(j: &Journal<P>) -> Self {
        let mut buckets = PVec::with_capacity(BUCKETS_MAX, j);
        for _ in 0..BUCKETS_MAX {
            buckets.push(PRefCell::new(PVec::new()), j)
        }
        Self {
            buckets,
            values: PVec::new(),
        }
    }

    pub fn get(&self, key: K) -> Option<&V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            let e = e.borrow();
            if e.0 == key {
                return Some(unsafe { &*(&*self.values[e.1].borrow() as *const V) });
            }
        }
        None
    }

    pub fn get_with_hash<Key>(&self, key: Key, key_hash: u64) -> Option<&V>
    where K: PartialEq<Key> {
        let index = (key_hash as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            let e = e.borrow();
            if e.0 == key {
                return Some(unsafe { &*(&*self.values[e.1].borrow() as *const V) });
            }
        }
        None
    }

    pub fn put(&mut self, key: K, val: V, j: &Journal<P>) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == key {
                *self.values[e.1].borrow_mut(j) = val;
                return;
            }
        }

        self.values.push(PRefCell::new(val), j);
        bucket.push(PRefCell::new((key, self.values.len() - 1)), j);
    }

    pub fn put_with_hash<Key>(&mut self, key: Key, key_hash: u64, val: V, j: &Journal<P>)
    where K: PartialEq<Key> + PFrom<Key, P> {
        let index = (key_hash as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == key {
                *self.values[e.1].borrow_mut(j) = val;
                return;
            }
        }

        self.values.push(PRefCell::new(val), j);
        bucket.push(PRefCell::new((K::pfrom(key, j), self.values.len() - 1)), j);
    }

    pub fn get_or_insert<F: FnOnce()->V>(&mut self, key: K, f: F, j: &Journal<P>) -> &V {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            let e = e.borrow();
            if e.0 == key {
                return unsafe { &*(&*self.values[e.1].borrow() as *const V) };
            }
        }
        self.put_once(key, f(), j)
    }

    pub fn put_once(&mut self, key: K, val: V, j: &Journal<P>) -> &V {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);
        self.values.push(PRefCell::new(val), j);
        bucket.push(PRefCell::new((key, self.values.len() - 1)), j);
        let new = self.values.last().unwrap().borrow();
        unsafe { &*(&*new as *const V) }
    }

    pub fn update_with<F: FnOnce(&V) -> V>(&mut self, key: &K, j: &Journal<P>, f: F)
    where
        V: Default,
        K: PClone<P>,
    {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == *key {
                *self.values[e.1].borrow_mut(j) = f(&self.values[e.1].borrow());
                return;
            }
        }

        self.values.push(PRefCell::new(f(&V::default())), j);
        bucket.push(
            PRefCell::new((key.pclone(j), self.values.len() - 1)),
            j,
        );
    }

    pub fn clear(&mut self, j: &Journal<P>) {
        for i in 0..BUCKETS_MAX {
            self.buckets[i].borrow_mut(j).clear();
        }
        self.values.clear();
    }

    pub fn is_empty(&self) -> bool {
        for i in 0..BUCKETS_MAX {
            if !self.buckets[i].borrow().is_empty() {
                return false;
            }
        }
        true
    }
}

impl<K: PSafe + Display, V: PSafe + Display + Copy, P: MemPool> Display for HashMap<K, V, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let mut vec = vec![];
        self.foreach(|word, freq| {
            vec.push((word.to_string(), freq.clone()));
        });
        vec.sort_by(|x, y| x.0.cmp(&y.0));
        for (word, freq) in vec {
            writeln!(f, "{:>32}: {}", word, freq)?;
        }
        Ok(())
    }
}