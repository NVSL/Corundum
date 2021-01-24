//! Random swaps between entries in an 8 GB array of integers between zero
//! and `ARRAY_SIZE`
//!
//! *Consistency invariant*: All integers should present onces in the array

extern crate corundum;

use corundum::default::*;
use rand::random;
use std::time::Instant;

type P = BuddyAlloc;

const ARRAY_SIZE: usize = 80;

struct Root {
    vec: PVec<PCell<usize>>,
}

impl RootObj<P> for Root {
    fn init(j: &Journal) -> Self {
        let mut vec = PVec::with_capacity(ARRAY_SIZE, j);
        for i in 0..ARRAY_SIZE {
            vec.push(PCell::new(i), j);
        }
        Root { vec }
    }
}

impl Root {
    fn verify(&self) -> bool {
        let mut vec = std::vec::Vec::<bool>::with_capacity(ARRAY_SIZE);
        for _ in 0..ARRAY_SIZE {
            vec.push(false);
        }
        for i in 0..ARRAY_SIZE {
            let v = self.vec[i].get();
            if vec[v] {
                return false;
            }
            vec[v] = true;
        }
        true
    }
}

fn main() {
    let root = P::open::<Root>("sps.pool", O_CFNE | O_1GB).unwrap();
    let mid = ARRAY_SIZE / 2;

    let start = Instant::now();
    for _ in 0..mid {
        P::transaction(|j| {
            let a = random::<usize>()% mid;
            let b = mid + random::<usize>() % mid;
            root.vec[a].swap(&root.vec[b], j);
        })
        .unwrap();
    }
    let duration = start.elapsed();
    println!("Time elapsed (PM): {:?}", duration);
    // println!("Memory Footprint: {} bytes", P::footprint());

    if root.verify() {
        println!("Verification successful");
    } else {
        println!("Verification unsuccessful");
        std::process::exit(-1);
    }

    use std::cell::Cell;
    let mut vec = std::vec::Vec::<Cell<usize>>::with_capacity(ARRAY_SIZE);
    for i in 0..ARRAY_SIZE {
        vec.push(Cell::new(i));
    }

    let start = Instant::now();
    for a in 0..mid {
        let b = mid + a;
        vec[a].swap(&vec[b]);
    }
    let duration = start.elapsed();
    println!("Time elapsed (DRAM): {:?}", duration);
}
