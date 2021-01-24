use std::fmt::Display;
use corundum::default::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const BUCKETS_MAX: usize = 16;

type P = BuddyAlloc;

type Bucket<K> = PVec<PRefCell<(K, usize)>>;

pub struct HashMap<K: PSafe, V: PSafe> {
    buckets: PVec<PRefCell<Bucket<K>>>,
    values: PVec<PCell<V>>,
}



impl<K: PSafe, V: PSafe + Copy> HashMap<K, V> {
    pub fn foreach<F: FnMut(&K, V) -> ()>(&self, mut f: F) {
        for i in 0..BUCKETS_MAX {
            for e in &*self.buckets[i].borrow() {
                let e = e.borrow();
                f(&e.0, self.values[e.1].get());
            }
        }
    }
}

impl<K: PSafe, V: PSafe + PClone<P>> HashMap<K, V>
where
    K: PartialEq + Hash,
    V: Copy,
{
    pub fn new(j: &Journal) -> Self {
        let mut buckets = PVec::with_capacity(BUCKETS_MAX, j);
        for _ in 0..BUCKETS_MAX {
            buckets.push(PRefCell::new(PVec::new()), j)
        }
        Self {
            buckets,
            values: PVec::new(),
        }
    }

    pub fn get(&self, key: K) -> Option<V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            let e = e.borrow();
            if e.0 == key {
                return Some(self.values[e.1].get());
            }
        }
        None
    }

    pub fn put(&mut self, key: K, val: V, j: &Journal) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == key {
                self.values[e.1].set(val, j);
                return;
            }
        }

        self.values.push(PCell::new(val), j);
        bucket.push(PRefCell::new((key, self.values.len() - 1)), j);
    }

    pub fn update_with<F: FnOnce(V) -> V>(&mut self, key: &K, j: &Journal, f: F)
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
                self.values[e.1].set(f(self.values[e.1].get()), j);
                return;
            }
        }

        self.values.push(PCell::new(f(V::default())), j);
        bucket.push(
            PRefCell::new((key.pclone(j), self.values.len() - 1)),
            j,
        );
    }

    pub fn clear(&mut self, j: &Journal) {
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

impl<K: PSafe + Display, V: PSafe + Display + Copy> Display for HashMap<K, V> {
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