use crate::Prog;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Debug, Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::vec::Vec;

const BUCKETS_MAX: usize = 16;

type Bucket<K> = Vec<RefCell<(K, usize)>>;

pub struct HashMap<K, V> {
    buckets: Vec<RefCell<Bucket<K>>>,
    values: RefCell<Vec<Cell<V>>>,
}

impl<K, V: Clone> HashMap<K, V>
where
    K: PartialEq + Hash,
    V: Copy,
{
    pub fn new() -> Self {
        let mut buckets = Vec::with_capacity(BUCKETS_MAX);
        for _ in 0..BUCKETS_MAX {
            buckets.push(RefCell::new(Vec::new()))
        }
        Self {
            buckets,
            values: RefCell::new(Vec::new()),
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

    pub fn put(&self, key: K, val: V) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % BUCKETS_MAX;
        let mut bucket = self.buckets[index].borrow_mut();

        for e in &*bucket {
            let e = e.borrow();
            if e.0 == key {
                self.values.borrow()[e.1].set(val);
                return;
            }
        }

        self.values.borrow_mut().push(Cell::new(val));
        bucket.push(RefCell::new((key, self.values.borrow().len() - 1)));
    }

    pub fn clear(&mut self) {
        for i in 0..BUCKETS_MAX {
            *self.buckets[i].borrow_mut() = Vec::new();
        }
        self.values.borrow_mut().clear();
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

impl<K: Debug, V: Debug> Debug for HashMap<K, V> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{:?}", self.buckets)
    }
}

impl<
        K: PartialEq + FromStr + Hash + Debug,
        V: Display + PartialEq + Copy + Clone + FromStr + Debug,
    > Prog for HashMap<K, V>
where
    <K as FromStr>::Err: Debug,
    <V as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let store = Self::new();
        f(&store)
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
                                self.put(key, val)
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
        println!("usage: store vhash [OPERATIONS]");
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
