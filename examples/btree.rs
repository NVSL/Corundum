//! Implementation of persistent binary search tree

use corundum::alloc::*;
use corundum::default::*;
use std::convert::TryInto;
use std::env;
use std::fmt::{Display, Error, Formatter};

type P = corundum::default::BuddyAlloc;
type Ptr = Option<Pbox<BTreeNode>>;

struct BTreeNode {
    key: i64,
    slots: [Ptr; 2],
    value: [u8; 10],
}

impl BTreeNode {
    pub fn new(key: i64, value: &str) -> Self {
        let mut value = value.as_bytes().to_vec();
        value.truncate(10);
        let mut vec = Vec::with_capacity(10);
        vec.append(&mut value);
        while vec.len() < 10 {
            vec.push(0);
        }

        Self {
            key,
            slots: [None, None],
            value: vec.as_slice().try_into().unwrap(),
        }
    }
}

impl Display for BTreeNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "{} {}",
            self.key,
            String::from_utf8(self.value.to_vec()).unwrap()
        )
    }
}

struct BTree {
    root: Ptr,
}

impl RootObj<P> for BTree {
    fn init(_j: &Journal) -> Self {
        Self { root: None }
    }
}

impl BTree {
    pub fn insert(&self, key: i64, value: &str) {
        let mut dst = &self.root;
        while let Some(node) = dst {
            dst = &node.slots[if key > node.key { 1 } else { 0 }];
        }
        let _ = Pbox::initialize(dst, BTreeNode::new(key, value));
    }

    pub fn find(&self, key: i64) -> Option<String> {
        let mut curr = &self.root;
        while let Some(node) = curr {
            if node.key == key {
                return Some(String::from_utf8(node.value.to_vec()).unwrap());
            } else {
                curr = &node.slots[if key > node.key { 1 } else { 0 }];
            }
        }
        None
    }

    fn foreach<F: Copy + FnOnce(&BTreeNode) -> ()>(node: &Ptr, f: F) {
        if let Some(node) = node {
            Self::foreach(&node.slots[0], f);
            f(node);
            Self::foreach(&node.slots[1], f);
        }
    }

    pub fn print(&self) {
        Self::foreach(&self.root, |p| {
            println!("{}", p);
        });
    }
}

fn main() {
    let args: Vec<std::string::String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "usage: {} file-name [p|r|s|i|f] [key/count] [value]",
            args[0]
        );
    } else {
        let root = P::open::<BTree>(&args[1], O_CFNE | O_1GB).unwrap();

        let op = args[2].chars().next().unwrap();

        match op {
            'p' => root.print(),
            'i' => {
                let key: i64 = args[3].parse().unwrap();
                let value = &args[4];
                root.insert(key, value);
            }
            'f' => {
                let key: i64 = args[3].parse().unwrap();
                if let Some(value) = root.find(key) {
                    println!("{}", value);
                } else {
                    println!("not found\n");
                }
            }
            's' => {
                let cnt: i64 = args[3].parse().unwrap();
                for i in 1..cnt {
                    root.insert(i, "test");
                }
            }
            'r' => {
                let cnt: i64 = args[3].parse().unwrap();
                for i in 1..cnt {
                    let _ = root.find(i);
                }
            }
            _ => {
                println!("Unknown command `{}`", args[2]);
            }
        }
    }
}
