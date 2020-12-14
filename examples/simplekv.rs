//! simplekv.rs -- a rust implementation of [simplekv example] from PMDK,
//! libpmemobj++ of simple kv which uses vector to hold values, string as a key
//! and array to hold buckets.
//!
//! [simplekv example]: https://github.com/pmem/libpmemobj-cpp/tree/master/examples/simplekv

#![allow(dead_code)]
#![allow(incomplete_features)]
#![feature(type_name_of_val)]
#![feature(const_in_array_repeat_expressions)]
#![feature(const_generics)]

use crndm::default::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::panic::RefUnwindSafe;

const BUCKETS_MAX: usize = 16;

type P = BuddyAlloc;

type Key = PString;
type Bucket = PVec<(Key, usize)>;
type BucketArray = PVec<PRefCell<Bucket>>;
type ValueVector<Value> = PVec<Value>;

pub struct KvStore<V: PSafe> {
    buckets: BucketArray,
    values: PRefCell<ValueVector<PCell<V>>>,
}

impl<V: PSafe> RootObj<P> for KvStore<V> {
    fn init(j: &Journal) -> Self {
        let mut buckets = PVec::with_capacity(BUCKETS_MAX, j);
        for _ in 0..BUCKETS_MAX {
            buckets.push(PRefCell::new(PVec::new(j), j), j)
        }
        Self {
            buckets,
            values: PRefCell::new(PVec::new(j), j),
        }
    }
}

impl<V: PSafe + Copy> KvStore<V>
where
    V: TxInSafe + RefUnwindSafe,
{
    pub fn get(&self, key: &str) -> Option<V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            if e.0 == key {
                let values = self.values.borrow();
                return Some(values[e.1].get());
            }
        }
        None
    }

    pub fn put(&self, key: &str, val: V) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            if e.0 == key {
                P::transaction(|j| {
                    let values = self.values.borrow();
                    values[e.1].set(val, j);
                })
                .unwrap();
                return;
            }
        }

        P::transaction(|j| {
            let key = PString::from_str(key, j);
            let mut values = self.values.borrow_mut(j);
            values.push(PCell::new(val, j), j);
            let mut bucket = self.buckets[index].borrow_mut(j);
            bucket.push((key, values.len() - 1), j);
        })
        .unwrap();
    }
}

pub fn main() {
    use std::env;
    use std::vec::Vec as StdVec;

    let args: StdVec<String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "usage: {} file-name [get key|put key value] | [burst get|put|putget count]",
            args[0]
        );
        return;
    }

    let root = P::open::<KvStore<i32>>(&args[1], O_CFNE | O_1GB).unwrap();

    if args[2] == String::from("get") && args.len() == 4 {
        println!("{:?}", root.get(&*args[3]))
    } else if args[2] == String::from("put") && args.len() == 5 {
        root.put(&*args[3], args[4].parse().unwrap())
    }
    if args[2] == String::from("burst")
        && (args[3] == String::from("put") || args[3] == String::from("putget"))
        && args.len() == 5
    {
        for i in 0..args[4].parse().unwrap() {
            let key = format!("key{}", i);
            root.put(&*key, i);
            if i == 500 {
                // To see what happens when it crashes in the middle of burst
                // put, uncomment the following line:
                // panic!("test");
            }
        }
    }
    if args[2] == String::from("burst")
        && (args[3] == String::from("get") || args[3] == String::from("putget"))
        && args.len() == 5
    {
        for i in 0..args[4].parse().unwrap() {
            let key = format!("key{}", i);
            root.get(&*key);
        }
    }
}
