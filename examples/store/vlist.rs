use crate::Prog;
use std::boxed::*;
use std::cell::RefCell;
use std::fmt::{Debug, Display, Error, Formatter};
use std::str::FromStr;
use std::vec::Vec;

type Link<T> = RefCell<Option<Box<Node<T>>>>;

#[derive(Clone)]
struct Node<T> {
    data: T,
    next: Link<T>,
}

impl<T: Clone> Node<T> {
    fn push_back(&self, data: T) {
        if let Some(next) = &*self.next.borrow() {
            next.push_back(data);
            return;
        }
        *self.next.borrow_mut() = Some(Box::new(Node {
            data,
            next: RefCell::new(None),
        }));
    }

    fn pop_back(&self) -> (T, bool) {
        let mut drop_next = false;
        let res = if let Some(next) = &*self.next.borrow() {
            if next.next.borrow().is_none() {
                drop_next = true;
                (next.data.clone(), false)
            } else {
                next.pop_back()
            }
        } else {
            (self.data.clone(), true)
        };
        if drop_next {
            self.next.replace(None);
        }
        res
    }
}

impl<T: Display> Display for Node<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(next) = &*self.next.borrow() {
            write!(fmt, "({})->{}", self.data, next)
        } else {
            write!(fmt, "({})", self.data)
        }
    }
}

pub struct List<T> {
    root: Link<T>,
}

impl<T: Clone> List<T> {
    pub fn push_front(&self, data: T) {
        *self.root.borrow_mut() = Some(Box::new(Node {
            data,
            next: self.root.clone(),
        }));
    }

    pub fn push_back(&self, data: T) {
        if let Some(ref root) = &*self.root.borrow() {
            root.push_back(data);
            return;
        }
        self.push_front(data)
    }

    pub fn pop_front(&self) -> Option<T> {
        let mut drop_root = false;
        let res = if let Some(root) = &*self.root.borrow() {
            drop_root = true;
            Some(root.data.clone())
        } else {
            None
        };
        if drop_root {
            let mut root = self.root.borrow_mut();
            let r = root.clone().unwrap();
            let next = r.next.borrow();
            if let Some(next) = &*next {
                *root = Some(next.clone());
            } else {
                *root = None;
            }
        }
        res
    }

    pub fn pop_back(&self) -> Option<T> {
        let mut drop_root = false;
        let res = if let Some(root) = &*self.root.borrow() {
            let (d, drop) = root.pop_back();
            drop_root = drop;
            Some(d)
        } else {
            None
        };
        if drop_root {
            *self.root.borrow_mut() = None;
        }
        res
    }

    pub fn clear(&self) {
        *self.root.borrow_mut() = None;
    }
}

impl<T: Display> Display for List<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(root) = &*self.root.borrow() {
            write!(fmt, "{}", root)
        } else {
            write!(fmt, "Empty")
        }
    }
}

impl<T: Display + Clone + FromStr + Debug> Prog for List<T>
where
    <T as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let store = Self {
            root: RefCell::new(None),
        };
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
                    } else if op == "push_back" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            self.push_back(n)
                        } else {
                            return false;
                        }
                    } else if op == "push_front" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            self.push_front(n)
                        } else {
                            return false;
                        }
                    } else if op == "pop_back" {
                        println!("{:?}", self.pop_back())
                    } else if op == "pop_front" {
                        println!("{:?}", self.pop_front())
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
        println!("usage: store vlist [OPERATIONS]");
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
