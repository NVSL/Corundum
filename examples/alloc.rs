use corundum::alloc::*;
use std::env;

type P = corundum::default::BuddyAlloc;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        println!("usage: {} file-name len count", args[0]);
        return;
    }

    let path = &args[1];
    let len: usize = args[2].parse().expect("expected an integer");
    let cnt: usize = args[3].parse().expect("expected an integer");

    let _pool = P::open_no_root(path, O_CFNE | O_1GB).unwrap();

    for _ in 0..cnt {
        unsafe {
            P::alloc(len);
        }
    }
}
