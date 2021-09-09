use crate::Prog;
use corundum::default::*;
use std::fmt::{Debug, Display, Error, Formatter};
use std::panic::*;
use std::str::FromStr;
use corundum::open_flags::*;

type P = Allocator;
type Link<T> = Prc<PRefCell<Option<Node<T>>>>;
pub trait NVData = PSafe + TxInSafe + TxOutSafe + UnwindSafe + RefUnwindSafe + Clone;

#[derive(Debug)]
struct Node<T: NVData> {
    data: PRefCell<T>,
    left: Link<T>,
    right: Link<T>,
}

impl<T: PartialOrd + NVData> Node<T> {
    pub fn search(&self, data: T) -> Option<T> {
        let d = self.data.borrow().clone();
        if data == d {
            Some(d)
        } else if data < d {
            if let Some(left) = &*self.left.borrow() {
                left.search(data)
            } else {
                None
            }
        } else {
            if let Some(right) = &*self.right.borrow() {
                right.search(data)
            } else {
                None
            }
        }
    }

    fn smallest(node: Link<T>, j: &Journal) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            if n.left.borrow().is_some() {
                Self::smallest(n.left.pclone(j), j)
            } else {
                node.pclone(j)
            }
        } else {
            node.pclone(j)
        }
    }

    fn largest(node: Link<T>, j: &Journal) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            if n.left.borrow().is_some() {
                Self::smallest(n.right.pclone(j), j)
            } else {
                node.pclone(j)
            }
        } else {
            node.pclone(j)
        }
    }

    fn loc(node: Link<T>, data: &T, j: &Journal) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            let d = &*n.data.borrow();
            if *data == *d {
                node.pclone(j)
            } else if *data < *d {
                Node::<T>::loc(n.left.pclone(j), data, j)
            } else {
                Node::<T>::loc(n.right.pclone(j), data, j)
            }
        } else {
            node.pclone(j)
        }
    }

    pub fn remove(node: Link<T>, data: T, j: &Journal) {
        let mut make_null = false;
        if let Some(node) = &*node.borrow_mut(j) {
            if data == *node.data.borrow() {
                make_null = node.right.borrow().is_none() && node.left.borrow().is_none();
                let succ = if node.right.borrow().is_some() {
                    Self::smallest(node.right.pclone(j), j)
                } else {
                    Self::largest(node.left.pclone(j), j)
                };
                if !make_null {
                    let succ_data = succ.borrow().as_ref().unwrap().data.borrow().clone();
                    *node.data.borrow_mut(j) = succ_data.clone();
                    Self::remove(succ, succ_data, j);
                }
            } else if data < *node.data.borrow() {
                Self::remove(node.left.pclone(j), data, j);
            } else {
                Self::remove(node.right.pclone(j), data, j);
            }
        }
        if make_null {
            *node.borrow_mut(j) = None;
        }
    }
}

impl<T: Display + PartialEq + NVData> Node<T> {
    pub fn print(&self, prefix: String, look: &Option<T>) -> String {
        let mut left_prefix = prefix.clone();
        left_prefix.push_str("│  ");
        let mut right_prefix = prefix.clone();
        right_prefix.push_str("   ");
        let mut res = if let Some(d) = &look {
            let my_d = &*self.data.borrow();
            if *d == *my_d {
                format!("\x1B[1;31m{}\x1B[0m\n", my_d)
            } else {
                format!("{}\n", my_d)
            }
        } else {
            format!("{}\n", self.data.borrow())
        };
        if let Some(left) = &*self.left.borrow() {
            res += &format!("{}├─ {}\n", prefix, left.print(left_prefix, look));
        } else {
            res += &format!("{}├─x\n", prefix);
        }
        if let Some(right) = &*self.right.borrow() {
            res += &format!("{}└─ {}", prefix, right.print(right_prefix, look));
        } else {
            res += &format!("{}└─x", prefix);
        }
        res
    }
}

impl<T: Display + PartialEq + NVData> Display for Node<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{}", self.print("".to_string(), &None))
    }
}

pub struct BST<T: NVData> {
    root: Link<T>,
}

impl<T: PartialOrd + NVData> BST<T> {
    pub fn insert(&self, data: T, j: &Journal) {
        let loc = Node::<T>::loc(self.root.pclone(j), &data, j);
        let mut loc = loc.borrow_mut(j);
        if loc.is_none() {
            *loc = Some(Node {
                data: PRefCell::new(data),
                left: Prc::new(PRefCell::new(None), j),
                right: Prc::new(PRefCell::new(None), j),
            });
        }
    }

    pub fn remove(&self, data: T, j: &Journal) {
        Node::<T>::remove(self.root.pclone(j), data, j)
    }

    pub fn search(&self, data: T) -> Option<T> {
        if let Some(root) = &*self.root.borrow() {
            root.search(data)
        } else {
            None
        }
    }

    pub fn clear(&self, j: &Journal) {
        *self.root.borrow_mut(j) = None;
    }
}

impl<T: PartialEq + Display + NVData> BST<T> {
    pub fn print(&self, look: &Option<T>) -> String {
        if let Some(root) = &*self.root.borrow() {
            format!("{}", root.print("".to_string(), look))
        } else {
            "Empty".to_string()
        }
    }
}

impl<T: Display + PartialEq + NVData> Display for BST<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{}", self.print(&None))
    }
}

impl<T: NVData> RootObj<P> for BST<T> {
    fn init(j: &Journal) -> Self {
        Self {
            root: Prc::new(PRefCell::new(None), j),
        }
    }
}

impl<T: Display + Debug + NVData + PartialOrd + FromStr> Prog for BST<T>
where
    <T as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let root = P::open::<Self>("/mnt/pmem0/bst.pool", O_CFNE).unwrap();
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
                    } else if op == "ins" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            P::transaction(|j| self.insert(n, j)).unwrap()
                        } else {
                            return false;
                        }
                    } else if op == "del" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            P::transaction(|j| self.remove(n, j)).unwrap()
                        } else {
                            return false;
                        }
                    } else if op == "c" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            println!("{:?}", self.search(n))
                        } else {
                            return false;
                        }
                    } else if op == "clear" {
                        P::transaction(|j| self.clear(j)).unwrap()
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
        println!("usage: store pbst [OPERATIONS]");
        println!("data type: {}", std::any::type_name::<T>());
        println!();
        println!("OPERATIONS:");
        println!("  ins data         Insert data");
        println!("  del data         Delete data");
        println!("  find data        Search for data");
        println!("  repeat n         Repeat the next operation n times");
        println!("  run file         Run a script file");
        println!("  clear            Delete all elements");
        println!("  print            Print the entire tree");
        println!("  help             Display help");
        println!();
    }
}
