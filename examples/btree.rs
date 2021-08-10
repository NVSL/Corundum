//! Implementation of persistent binary search tree

use corundum::alloc::*;
use corundum::*;
use std::env;
use std::fmt::{Display, Error, Formatter};

type P = corundum::default::Allocator;
//type P = Heap;
type Pbox<T> = corundum::boxed::Pbox<T, P>;
type Ptr = Option<Pbox<BTreeNode>>;

struct FixStr {
    chars: [u8; 32]
}

impl From<String> for FixStr {
    fn from(value: String) -> Self {
        let value = value.as_bytes();
        let mut chars = [0u8; 32];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &value[0],
                &mut chars[0] as *mut u8,
                value.len().min(32));
        }
        FixStr {
            chars
        }
    }
}

impl Display for FixStr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let s = String::from_utf8(self.chars.to_vec()).unwrap();
        write!(f, "{}", s)
    }
}

struct BTreeNode {
    key: i64,
    slots: [Ptr; 2],
    value: FixStr,
}

impl BTreeNode {
    pub fn new(key: i64, value: &str) -> Self {
        Self {
            key,
            slots: [None, None],
            value: FixStr::from(value.to_string()),
        }
    }
}

impl Display for BTreeNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {}", self.key, self.value)
    }
}

#[derive(Root)]
struct BTree {
    root: Ptr,
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
                return Some(node.value.to_string());
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

