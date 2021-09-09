use corundum::default::*;
use corundum::open_flags::*;

type P = Allocator;

fn main() {
    use std::env;
    use std::vec::Vec as StdVec;

    let args: StdVec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} file-name", args[0]);
        return;
    }

    let _pool = P::open_no_root(&args[1], O_READINFO).unwrap();
    P::print_info();
}
