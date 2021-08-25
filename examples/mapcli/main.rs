#![feature(maybe_uninit_uninit_array)]
#![allow(dead_code)]

mod hashmap;
mod map;
mod skiplist;

use crate::map::*;
use hashmap::*;
use corundum::default::*;
use skiplist::*;
use std::env;
use std::io::prelude::*;
use std::io::{stdin, stdout};

fn help() {
    println!("h - help");
    println!("i $value - insert $value");
    println!("r $value - remove $value");
    println!("c $value - check $value, returns 0/1");
    println!("n $value - insert $value random values");
    println!("C - clear all items");
    println!("p - print all values");
    println!("d - print debug info");
    println!("b [$value] - rebuild $value (default: 1) times");
    println!("q - quit");
    println!();
    println!("usage: {} bytes", Allocator::used());
}

fn unknown_command(cmd: &str) {
    eprintln!("unknown command '{}', use 'h' for help", cmd);
}

fn print_all<T: Map<u64, u64>>(map: &T) {
    map.foreach(|key, _val| {
        print!("{} ", key);
        false
    });
    println!();
    stdout().flush().unwrap();
}

fn str_insert<T: Map<u64, u64>>(map: &T, buf: &mut String) {
    buf.pop();
    let key: u64 = buf.trim().parse().expect("insert: invalid syntax");
    map.insert(key, 0);
}

fn str_remove<T: Map<u64, u64>>(map: &T, buf: &mut String) {
    buf.pop();
    let key: u64 = buf.trim().parse().expect("insert: invalid syntax");
    map.remove(key);
}

fn str_insert_random<T: Map<u64, u64>>(map: &T, buf: &mut String) {
    buf.pop();
    let val: u64 = buf.trim().parse().expect("random insert: invalid syntax");
    for _ in 0..val {
        let key = rand::random::<u64>();
        map.insert(key, 0);
    }
}

fn str_check<T: Map<u64, u64>>(map: &T, buf: &mut String) {
    buf.pop();
    let key: u64 = buf.trim().parse().expect("check: invalid syntax");
    println!("{}", map.lookup(key) as u8);
}

fn perform<T: 'static + Map<u64, u64> + RootObj<P> + PSafe>(path: &str) {
    let map = P::open::<T>(path, O_CFNE | O_16GB).unwrap();
    let mut buf = String::new();

    print!("$ ");
    stdout().flush().unwrap();
    while let Ok(_) = stdin().read_line(&mut buf) {
        if buf.is_empty() { break }
        match buf.remove(0) as char {
            'i' => str_insert(&*map, &mut buf),
            'c' => str_check(&*map, &mut buf),
            'r' => str_remove(&*map, &mut buf),
            'n' => str_insert_random(&*map, &mut buf),
            'C' => map.clear(),
            'p' => print_all(&*map),
            'h' => help(),
            'q' => return,
            '\n' => continue,
            s => unknown_command(&s.to_string()),
        }
        print!("$ ");
        stdout().flush().unwrap();
        buf.clear();
    }
}

fn vperform<T: Map<u64, u64> + Default>() {
    let map = T::default();
    let mut buf = String::new();

    print!("$ ");
    stdout().flush().unwrap();
    while let Ok(_) = stdin().read_line(&mut buf) {
        match buf.remove(0) as char {
            'i' => str_insert(&map, &mut buf),
            'c' => str_check(&map, &mut buf),
            'r' => str_remove(&map, &mut buf),
            'n' => str_insert_random(&map, &mut buf),
            'p' => print_all(&map),
            'h' => help(),
            'q' => return,
            '\n' => continue,
            s => unknown_command(&s.to_string()),
        }
        print!("$ ");
        stdout().flush().unwrap();
        buf.clear();
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        println!(
            concat!(
                "usage: {} ",
                "hashmap_tx|hashmap_atomic|hashmap_rp|",
                "ctree|btree|rtree|rbtree|skiplist ",
                "file-name"
            ),
            args[0]
        );
        return;
    }

    let typ = &args[1];
    let path = &args[2];
    if typ == "hashmap_tx" {
        perform::<HashmapTx>(path)
    } else if typ == "hashmap_atomic" {
        perform::<HashmapAtomic>(path)
    } else if typ == "hashmap_rp" {
        perform::<HashmapRp>(path)
    } else if typ == "ctree" {
        perform::<CTree>(path)
    } else if typ == "btree" {
        perform::<BTree<u64>>(path)
        // unimplemented!()
    } else if typ == "vbtree" {
        // vperform::<VBTree<u64>>()
    } else if typ == "rtree" {
        vperform::<RTree<u64, u64>>()
    } else if typ == "rbtree" {
        perform::<RbTree>(path)
    } else if typ == "skiplist" {
        perform::<Skiplist>(path)
    } else {
        panic!("invalid container type -- '{}'", path);
    }
}
