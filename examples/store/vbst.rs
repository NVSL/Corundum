use crate::Prog;
use std::cell::RefCell;
use std::fmt::{Debug, Display, Error, Formatter};
use std::rc::Rc;
use std::str::FromStr;

type Link<T> = Rc<RefCell<Option<Node<T>>>>;

#[derive(Clone, Debug)]
struct Node<T> {
    data: RefCell<T>,
    left: Link<T>,
    right: Link<T>,
}

impl<T: PartialOrd + Clone> Node<T> {
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

    fn smallest(node: Link<T>) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            if n.left.borrow().is_some() {
                Self::smallest(n.left.clone())
            } else {
                node.clone()
            }
        } else {
            node.clone()
        }
    }

    fn largest(node: Link<T>) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            if n.left.borrow().is_some() {
                Self::smallest(n.right.clone())
            } else {
                node.clone()
            }
        } else {
            node.clone()
        }
    }

    fn loc(node: Link<T>, data: &T) -> Link<T> {
        if let Some(n) = &*node.borrow() {
            let d = &*n.data.borrow();
            if *data == *d {
                node.clone()
            } else if *data < *d {
                Node::<T>::loc(n.left.clone(), data)
            } else {
                Node::<T>::loc(n.right.clone(), data)
            }
        } else {
            node.clone()
        }
    }

    pub fn remove(node: Link<T>, data: T) {
        let mut make_null = false;
        if let Some(node) = &*node.borrow_mut() {
            if data == *node.data.borrow() {
                make_null = node.right.borrow().is_none() && node.left.borrow().is_none();
                let succ = if node.right.borrow().is_some() {
                    Self::smallest(node.right.clone())
                } else {
                    Self::largest(node.left.clone())
                };
                if !make_null {
                    let succ_data = succ.borrow().as_ref().unwrap().data.borrow().clone();
                    *node.data.borrow_mut() = succ_data.clone();
                    Self::remove(succ, succ_data);
                }
            } else if data < *node.data.borrow() {
                Self::remove(node.left.clone(), data);
            } else {
                Self::remove(node.right.clone(), data);
            }
        }
        if make_null {
            *node.borrow_mut() = None;
        }
    }
}

impl<T: Display + PartialEq> Node<T> {
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

impl<T: Display + PartialEq> Display for Node<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{}", self.print("".to_string(), &None))
    }
}

pub struct BST<T> {
    root: Link<T>,
}

impl<T: PartialOrd + Clone> BST<T> {
    pub fn insert(&self, data: T) {
        let loc = Node::<T>::loc(self.root.clone(), &data);
        let mut loc = loc.borrow_mut();
        if loc.is_none() {
            *loc = Some(Node {
                data: RefCell::new(data),
                left: Rc::new(RefCell::new(None)),
                right: Rc::new(RefCell::new(None)),
            });
        }
    }

    pub fn remove(&self, data: T) {
        Node::<T>::remove(self.root.clone(), data)
    }

    pub fn search(&self, data: T) -> Option<T> {
        if let Some(root) = &*self.root.borrow() {
            root.search(data)
        } else {
            None
        }
    }

    pub fn clear(&self) {
        *self.root.borrow_mut() = None;
    }
}

impl<T: PartialEq + Display> BST<T> {
    pub fn print(&self, look: &Option<T>) -> String {
        if let Some(root) = &*self.root.borrow() {
            format!("{}", root.print("".to_string(), look))
        } else {
            "Empty".to_string()
        }
    }
}

impl<T: Display + PartialEq> Display for BST<T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        write!(fmt, "{}", self.print(&None))
    }
}

impl<T: Display + Debug + Clone + PartialOrd + FromStr> Prog for BST<T>
where
    <T as FromStr>::Err: Debug,
{
    fn perform<F: FnOnce(&Self)>(f: F) {
        let store = Self {
            root: Rc::new(RefCell::new(None)),
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
                    } else if op == "ins" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            self.insert(n)
                        } else {
                            return false;
                        }
                    } else if op == "del" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            self.remove(n)
                        } else {
                            return false;
                        }
                    } else if op == "find" {
                        if let Some(n) = Self::next(&args, &mut i) {
                            let n: T = n
                                .parse()
                                .expect(&format!("Expected a(n) {}", std::any::type_name::<T>()));
                            println!("{}", self.print(&self.search(n)))
                        } else {
                            return false;
                        }
                    } else if op == "clear" {
                        self.clear()
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
        println!("usage: store vbst [OPERATIONS]");
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
