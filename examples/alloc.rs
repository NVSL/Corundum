use corundum::alloc::*;
use std::env;

type P = corundum::default::BuddyAlloc;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 5 {
        println!("usage: {} [block-size] [count/thread] [threads]", args[0]);
        return;
    }

    let len: usize = args[1].parse().expect("expected an integer");
    let cnt: usize = args[2].parse().expect("expected an integer");
    let thr: usize = args[3].parse().expect("expected an integer");

    let _pool = P::open_no_root("/mnt/pmem/crndm.pool", O_CF | O_8GB).unwrap();

    println!("Allocating {} block(s) of {} byte(s) in {} thread(s)", cnt*thr, len, thr);

    let mut ts = vec!();
    for _ in 0..thr {
        ts.push(std::thread::spawn(move || {
            for _ in 0..cnt {
                unsafe {
                    P::alloc(len);
                }
            }
        }));
    }
    
    for t in ts {
        t.join().unwrap();
    }
}
