use corundum::alloc::*;
use std::env;

type P = corundum::default::BuddyAlloc;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        println!("usage: {} file-name len", args[0]);
        return;
    }

    let path = &args[1];
    let len: usize = args[2].parse().expect("expected an integer");

    let _pool = P::open_no_root(path, O_CFNE | O_1GB).unwrap();

    let layout = std::alloc::Layout::new::<i32>();
    for _ in 0..len {
        unsafe {
            P::alloc(layout.size());
        }
    }
}
