use crate::Prog;
use corundum::default::*;
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Debug, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::panic::*;
use std::str::FromStr;

const BUCKETS_MAX: usize = 16;

type P = BuddyAlloc;
pub trait NVData =
    PSafe + TxInSafe + UnwindSafe + RefUnwindSafe + PartialEq + FromStr + Hash + Debug;

type Bucket<K> = PVec<PRefCell<(K, usize)>>;

pub struct HashMap<K: PSafe, V: PSafe> {
    buckets: PVec<PRefCell<Bucket<K>>>,
    values: PRefCell<PVec<PCell<V>>>,
}

impl<K: PSafe, V: PSafe> HashMap<K, V>
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
            values: PRefCell::new(PVec::new()),
        }
    }

    pub fn get(&self, key: K) -> Option<V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;

        for e in &*self.buckets[index].borrow() {
            let e = e.borrow();
            if e.0 == key {
                return Some(self.values.borrow()[e.1].get());
            }
        }
        None
    }

    pub fn put(&self, key: K, val: V, j: &Journal) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut(j);

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == key {
                self.values.borrow()[e.1].set(val, j);
                return;
            }
        }

        self.values.borrow_mut(j).push(PCell::new(val), j);
        bucket.push(PRefCell::new((key, self.values.borrow().len() - 1)), j);
    }

    pub fn clear(&self, j: &Journal) {
        for i in 0..BUCKETS_MAX {
            *self.buckets[i].borrow_mut(j) = PVec::new();
        }
        self.values.borrow_mut(j).clear();
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

impl<K: PSafe + Debug, V: PSafe + Debug> Debug for HashMap<K, V> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{:?}", self.buckets)
    }
}

impl<K: NVData, V: NVData + Copy> RootObj<P> for HashMap<K, V> {
    fn init(j: &Journal) -> Self {
        Self::new(j)
    }
}

impl<K: NVData, V: NVData + Copy> Prog for HashMap<K, V>
where
    <K as FromStr>::Err: Debug,
    <V as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let root = P::open::<Self>("hash.pool", O_CFNE).unwrap();
        f(&root)
    }

    fn exec(&self, args: Vec<String>) -> bool {
        if args.len() < 2 {
            Self::help();
        } else {
            let mut i = 2;
            while i < args.len() {
                if let Some(op) = Self::next(&args, &mut i) {
                    if op == "help" {
                        Self::help()
                    } else if op == "repeat" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: usize = n.parse().expect("Expected an integer");
                            if !self.repeat(&args, i, n) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if op == "put" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let key: K = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<K>()));
                            if let Some(n) = Self::next(&args, &mut i) {
                                let val: V = n.parse().expect(&format!(
                                    "Expected a(n) {}",
                                    std::any::type_name::<V>()
                                ));
                                P::transaction(|j| self.put(key, val, j)).unwrap();
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if op == "get" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let key: K = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<K>()));
                            println!("{:?}", self.get(key))
                        } else {
                            return false;
                        }
                    } else if op == "run" {
                        if let Some(filename) = Self::next(&args, &mut i) {
                            return self.run(&filename);
                        } else {
                            return false;
                        }
                    } else if op == "print" {
                        println!("{:#?}", self)
                    } else if op == "help" {
                        Self::help()
                    }
                } else {
                    return true;
                }
            }
        }
        true
    }

    fn help() {
        println!("usage: store phash [OPERATIONS]");
        println!("key type: {}", std::any::type_name::<K>());
        println!("value type: {}", std::any::type_name::<V>());
        println!();
        println!("OPERATIONS:");
        println!("  put key data     Put (key, data) to the table");
        println!("  get key          Read data from the table given a key");
        println!("  repeat n         Repeat the next operation n times");
        println!("  run file         Run a script file");
        println!("  clear            Delete all elements");
        println!("  print            Print the entire table");
        println!("  help             Display help");
        println!();
    }
}
