use crate::Prog;
use crndm::default::*;
use std::fmt::{Debug, Display, Error, Formatter};
use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use std::str::FromStr;

type P = BuddyAlloc;
type Link<T> = PRefCell<Option<Pbox<Node<T>>>>;
pub trait NVData = PSafe + TxInSafe + TxOutSafe + UnwindSafe + RefUnwindSafe + Clone;

struct Node<T: NVData> {
    data: T,
    next: Link<T>,
}

impl<T: NVData> Node<T> {
    fn push_back(&self, data: T, j: &Journal) {
        if let Some(next) = &*self.next.borrow() {
            next.push_back(data, j);
            return;
        }
        *self.next.borrow_mut(j) = Some(Pbox::new(
            Node {
                data,
                next: PRefCell::new(None, j),
            },
            j,
        ));
    }

    fn pop_back(&self, j: &Journal) -> (T, bool) {
        let mut drop_next = false;
        let res = if let Some(next) = &*self.next.borrow() {
            if next.next.borrow().is_none() {
                drop_next = true;
                (next.data.clone(), false)
            } else {
                next.pop_back(j)
            }
        } else {
            (self.data.clone(), true)
        };
        if drop_next {
            self.next.replace(None, j);
        }
        res
    }
}

impl<T: NVData + Display> Display for Node<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(next) = &*self.next.borrow() {
            write!(fmt, "({})->{}", self.data, next)
        } else {
            write!(fmt, "({})", self.data)
        }
    }
}

impl<T: NVData> PClone<P> for Node<T> {
    fn pclone(&self, j: &Journal) -> Self {
        Self {
            data: self.data.clone(),
            next: self.next.pclone(j),
        }
    }
}

pub struct List<T: NVData> {
    root: Link<T>,
}

impl<T: NVData> List<T> {
    pub fn push_front(&self, data: T, j: &Journal) {
        *self.root.borrow_mut(j) = Some(Pbox::new(
            Node {
                data,
                next: self.root.pclone(j),
            },
            j,
        ));
    }

    pub fn push_back(&self, data: T, j: &Journal) {
        if let Some(ref root) = &*self.root.borrow() {
            root.push_back(data, j);
            return;
        }
        self.push_front(data, j);
    }

    pub fn pop_front(&self, j: &Journal) -> Option<T> {
        let mut drop_root = false;
        let res = if let Some(root) = &*self.root.borrow() {
            drop_root = true;
            Some(root.data.clone())
        } else {
            None
        };
        if drop_root {
            let mut root = self.root.borrow_mut(j);
            let r = root.pclone(j).unwrap();
            let next = r.next.borrow();
            if let Some(next) = &*next {
                *root = Some(next.pclone(j));
            } else {
                *root = None;
            }
        }
        res
    }

    pub fn pop_back(&self, j: &Journal) -> Option<T> {
        let mut drop_root = false;
        let res = if let Some(root) = &*self.root.borrow() {
            let (d, drop) = root.pop_back(j);
            drop_root = drop;
            Some(d)
        } else {
            None
        };
        if drop_root {
            *self.root.borrow_mut(j) = None;
        }
        res
    }

    pub fn clear(&self, j: &Journal) {
        *self.root.borrow_mut(j) = None;
    }
}

impl<T: NVData + Display> Display for List<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(root) = &*self.root.borrow() {
            write!(fmt, "{}", root)
        } else {
            write!(fmt, "Empty")
        }
    }
}

impl<T: NVData> RootObj<P> for List<T> {
    fn init(j: &Journal) -> Self {
        Self {
            root: PRefCell::new(None, j),
        }
    }
}

impl<T: 'static + NVData + Display + FromStr + Debug> Prog for List<T>
where
    <T as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let root = P::open::<Self>("list.pool", O_CFNE).unwrap();
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
                    } else if op == "push_back" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            P::transaction(|j| self.push_back(n, j)).unwrap();
                        } else {
                            return false;
                        }
                    } else if op == "push_front" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            P::transaction(|j| self.push_front(n, j)).unwrap();
                        } else {
                            return false;
                        }
                    } else if op == "pop_back" {
                        P::transaction(|j| println!("{:?}", self.pop_back(j))).unwrap();
                    } else if op == "pop_front" {
                        P::transaction(|j| println!("{:?}", self.pop_front(j))).unwrap();
                    } else if op == "run" {
                        if let Some(filename) = Self::next(&args, &mut i) {
                            return self.run(&filename);
                        } else {
                            return false;
                        }
                    } else if op == "print" {
                        println!("{}", self)
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
        println!("usage: store plist [OPERATIONS]");
        println!("data type: {}", std::any::type_name::<T>());
        println!();
        println!("OPERATIONS:");
        println!("  push_back data   Push data to the tail");
        println!("  push_front data  Push data to the head");
        println!("  pop_back         Pop an element from the tail");
        println!("  pop_front        Pop an element from the head");
        println!("  repeat n         Repeat the next operation n times");
        println!("  run file         Run a script file");
        println!("  clear            Delete all elements");
        println!("  print            Print the entire list");
        println!("  help             Display help");
        println!();
    }
}
