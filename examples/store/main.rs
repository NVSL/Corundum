#![allow(dead_code)]
#![feature(trait_alias)]

// 1. Linked List
// 2. Binary Search Tree
// 3. HashMap /w Vec

mod pbst;
mod phash;
mod plist;

mod vbst;
mod vhash;
mod vlist;

use std::env;

pub trait Prog {
    fn perform<F: FnOnce(&Self)>(f: F);
    fn exec(&self, args: Vec<String>) -> bool;
    fn help();

    fn next(args: &Vec<String>, i: &mut usize) -> Option<String> {
        if *i < args.len() {
            *i += 1;
            Some(args[*i - 1].clone())
        } else {
            None
        }
    }

    fn repeat(&self, args: &Vec<String>, i: usize, mut n: usize) -> bool {
        let mut v = args[i..].to_vec().clone();
        v.insert(0, "nop".to_string());
        v.insert(0, "nop".to_string());
        while n > 1 {
            if !self.exec(v.clone()) {
                return false;
            }
            n -= 1;
        }
        true
    }

    fn run(&self, filename: &str) -> bool {
        let contents = std::fs::read_to_string(filename).unwrap();
        let mut args: Vec<String> = contents.split_whitespace().map(|x| x.to_string()).collect();
        args.insert(0, "nop".to_string());
        args.insert(0, "nop".to_string());
        self.exec(args)
    }
}

fn perform<T: Prog>(args: Vec<String>) {
    T::perform(|store| {
        store.exec(args);
    });
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!(
            "usage: {} [vlist|plist|vbst|pbst|vhash|phash] [OPERATION]",
            args[0]
        )
    } else {
        let tp = &args[1];
        if tp == "vlist" {
            perform::<vlist::List<i32>>(args)
        } else if tp == "plist" {
            perform::<plist::List<i32>>(args)
        } else if tp == "vbst" {
            perform::<vbst::BST<i32>>(args)
        } else if tp == "pbst" {
            perform::<pbst::BST<i64>>(args)
        } else if tp == "vhash" {
            perform::<vhash::HashMap<i32, i32>>(args)
        } else if tp == "phash" {
            perform::<phash::HashMap<i32, i32>>(args)
        }
    }
}
