use corundum::stm::*;
use corundum::default::*;
use corundum::stat::*;
use std::time::Instant;

type P = BuddyAlloc;

fn main() {
    use std::env;
    use std::vec::Vec as StdVec;

    let args: StdVec<String> = env::args().collect();

    if args.len() < 3 {
        println!("usage: {} file-name iterations", args[0]);
        return;
    }

    let sizes = vec![1, 8, 32, 128, 512];
    let cnt = args[2].parse::<usize>().expect("Expected a number");

    let _pool = P::open_no_root(&args[1], O_CFNE | O_32GB).unwrap();
    for _ in 0..cnt {
        // Warm-up the allocator
        P::transaction(|_| {
            let s = 8 + rand::random::<usize>() % 1000;
            unsafe { P::alloc(s); }
        }).unwrap();
    }
    for _ in 0..cnt {
        measure!("TxNop".to_string(), {
            P::transaction(|_| {}).unwrap();
        });
    }
    P::transaction(|_| {
        for _ in 0..cnt {
            for s in &sizes {
                let s = *s * 8;
                measure!(format!("Alloc({})", s), {
                    unsafe{ P::alloc(s); }
                });
            }
        }
    }).unwrap();

    P::transaction(|_| {
        let mut blks = vec![];
        for _ in 0..cnt {
            for s in &sizes {
                unsafe{ blks.push(P::alloc(*s * 8)); }
            }
        }
        for (ptr, _, len) in &blks {
            measure!(format!("Dealloc({})", len), {
                unsafe{ P::dealloc(*ptr, *len); }
            });
        }
    }).unwrap();

    P::transaction(|j| {
        for _ in 0..cnt {
            let mut b = Pbox::new(10, j);
            let v;
            measure!("Deref".to_string(), {
                v = *b;
            });
            measure!("DerefMut".to_string(), {
                *b = 20;
            });
            if v < 0 {
                println!("unreachable {}", v);
            }
        }
    }).unwrap();

    P::transaction(|j| unsafe {
        for _ in 0..cnt {
            let b = Pbox::new(0u64, j);
            measure!("DataLog(8)".to_string(), {
                (*b).take_log(j, Notifier::None);
            });
            let b = Pbox::new([0u64;8], j);
            measure!("DataLog(64)".to_string(), {
                (*b).take_log(j, Notifier::None);
            });
            let b = Pbox::new([0u64;32], j);
            measure!("DataLog(2K)".to_string(), {
                (*b).take_log(j, Notifier::None);
            });
            let b = Pbox::new([0u64;128], j);
            measure!("DataLog(8K)".to_string(), {
                (*b).take_log(j, Notifier::None);
            });
            let b = Pbox::new([0u64;512], j);
            measure!("DataLog(32K)".to_string(), {
                (*b).take_log(j, Notifier::None);
            });
        }
        j.ignore();
    }).unwrap();

    P::transaction(|j| unsafe {
        for _ in 0..cnt {
            let (_, off, len) = P::alloc(8);
            measure!("DropLog(8)".to_string(), {
                Log::drop_on_commit(off, len, j);
            });
            let (_, off, len) = P::alloc(64);
            measure!("DropLog(64)".to_string(), {
                Log::drop_on_commit(off, len, j);
            });
            let (_, off, len) = P::alloc(2048);
            measure!("DropLog(2K)".to_string(), {
                Log::drop_on_commit(off, len, j);
            });
            let (_, off, len) = P::alloc(8*1024);
            measure!("DropLog(8K)".to_string(), {
                Log::drop_on_commit(off, len, j);
            });
            let (_, off, len) = P::alloc(32*1024);
            measure!("DropLog(32K)".to_string(), {
                Log::drop_on_commit(off, len, j);
            });
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Pbox::new(0u64, j);
        for _ in 0..cnt {
            let cpy;
            measure!("Pbox:clone".to_string(), {
                cpy = b.pclone(j);
            });
            if *cpy > 10 {
                println!("unreachable {}", cpy);
            }
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        for _ in 0..cnt {
            let cpy;
            measure!("Prc:clone".to_string(), {
                cpy = b.pclone(j);
            });
            if *cpy > 10 {
                println!("unreachable {}", cpy);
            }
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        for _ in 0..cnt {
            let cpy;
            measure!("Parc:clone".to_string(), {
                cpy = b.pclone(j);
            });
            if *cpy > 10 {
                println!("unreachable {}", cpy);
            }
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        for _ in 0..cnt {
            let dn;
            measure!("Prc:downgrade".to_string(), {
                dn = Prc::downgrade(&b, j);
            });
            let up;
            measure!("Prc:upgrade".to_string(), {
                up = dn.upgrade(j).unwrap();
            });
            if *up > 10 {
                println!("unreachable {}", up);
            }
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        for _ in 0..cnt {
            let dn;
            measure!("Parc:downgrade".to_string(), {
                dn = Parc::downgrade(&b, j);
            });
            let up;
            measure!("Parc:upgrade".to_string(), {
                up = dn.upgrade(j).unwrap();
            });
            let v;
            measure!("Parc:demote".to_string(), {
                unsafe { v = Parc::unsafe_demote(&b); }
            });
            let p;
            measure!("Parc:promote".to_string(), {
                p = v.promote(j).unwrap();
            });
            if *up > 10 {
                println!("unreachable {}", up);
                println!("unreachable {}", p);
            }
        }
    }).unwrap();

    for _ in 0..cnt {
        for s in &sizes {
            let layout = std::alloc::Layout::from_size_align(*s * 8, 4).unwrap();
            measure!(format!("malloc({})", *s * 8), {
                unsafe{ std::alloc::alloc(layout); }
            });
        }
    }

    eprintln!("{}", report());
}
