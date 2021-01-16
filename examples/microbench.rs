#![feature(asm)]

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
    measure!("TxNop".to_string(), cnt, {
        for _ in 0..cnt {
            P::transaction(|_| {unsafe { asm!("nop"); }}).unwrap();
        }
    });
    P::transaction(|_| {
        for s in &sizes {
            let s = *s * 8;
            measure!(format!("Alloc({})", s), cnt, {
                for _ in 0..cnt {
                    unsafe{ P::alloc(s); }
                }
            });
        }
    }).unwrap();

    P::transaction(|_| {
        for s in &sizes {
            let mut blks = vec![];
            for _ in 0..cnt {
                unsafe{ blks.push(P::alloc(*s * 8)); }
            }
            measure!(format!("Dealloc({})^", *s), cnt, {
                for i in 0..cnt {
                    unsafe{ P::dealloc(blks[i].0, *s); }
                }
            });
        }
    }).unwrap();

    P::transaction(|j| {
        let mut b = Pbox::new(10, j);
        let mut v = 0;
        measure!("Deref".to_string(), cnt, {
            for _ in 0..cnt {
                v += *b;
            }
        });
        measure!("DerefMut".to_string(), cnt, {
            for _ in 0..cnt {
                *b += 20;
            }
        });
        if v < 0 {
            println!("unreachable {}", v);
        }
    }).unwrap();

    P::transaction(|j| unsafe {
        let b = Pbox::new(0u64, j);
        measure!("DataLog(8)".to_string(), cnt, {
            for _ in 0..cnt {
                (*b).take_log(j, Notifier::None);
            }
        });
        let b = Pbox::new([0u64;8], j);
        measure!("DataLog(64)".to_string(), cnt, {
            for _ in 0..cnt {
                (*b).take_log(j, Notifier::None);
            }
        });
        let b = Pbox::new([0u64;32], j);
        measure!("DataLog(2K)".to_string(), cnt, {
            for _ in 0..cnt {
                (*b).take_log(j, Notifier::None);
            }
        });
        let b = Pbox::new([0u64;128], j);
        measure!("DataLog(8K)".to_string(), cnt, {
            for _ in 0..cnt {
                (*b).take_log(j, Notifier::None);
            }
        });
        let b = Pbox::new([0u64;512], j);
        measure!("DataLog(32K)".to_string(), cnt, {
            for _ in 0..cnt {
                (*b).take_log(j, Notifier::None);
            }
        });
        j.ignore();
    }).unwrap();

    P::transaction(|j| unsafe {
        let (_, off, len) = P::alloc(8);
        measure!("DropLog(8)".to_string(), cnt, {
            for _ in 0..cnt {
                Log::drop_on_commit(off, len, j);
            }
        });
        let (_, off, len) = P::alloc(64);
        measure!("DropLog(64)".to_string(), cnt, {
            for _ in 0..cnt {
                Log::drop_on_commit(off, len, j);
            }
        });
        let (_, off, len) = P::alloc(2048);
        measure!("DropLog(2K)".to_string(), cnt, {
            for _ in 0..cnt {
                Log::drop_on_commit(off, len, j);
            }
        });
        let (_, off, len) = P::alloc(8*1024);
        measure!("DropLog(8K)".to_string(), cnt, {
            for _ in 0..cnt {
                Log::drop_on_commit(off, len, j);
            }
        });
        let (_, off, len) = P::alloc(32*1024);
        measure!("DropLog(32K)".to_string(), cnt, {
            for _ in 0..cnt {
                Log::drop_on_commit(off, len, j);
            }
        });
        j.ignore();
    }).unwrap();

    P::transaction(|j| {
        let b = Pbox::new(0u64, j);
        let mut vec = Vec::<Pbox<u64>>::with_capacity(cnt);
        measure!("Pbox:clone*".to_string(), cnt, {
            for _ in 0..cnt {
                vec.push(b.pclone(j));
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        let mut vec = Vec::<Prc<u64>>::with_capacity(cnt);
        measure!("Prc:clone*".to_string(), cnt, {
            for _ in 0..cnt {
                vec.push(b.pclone(j));
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        let mut vec = Vec::<Parc<u64>>::with_capacity(cnt);
        measure!("Parc:clone*".to_string(), cnt, {
            for _ in 0..cnt {
                vec.push(b.pclone(j));
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        let mut vec = Vec::<prc::PWeak<u64>>::with_capacity(cnt);
        measure!("Prc:downgrade*".to_string(), cnt, {
            for _ in 0..cnt {
                vec.push(Prc::downgrade(&b, j));
            }
        });
        measure!("Prc:upgrade^".to_string(), cnt, {
            for i in 0..cnt {
                vec[i].upgrade(j).unwrap();
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        let mut pvec = Vec::<parc::PWeak<u64>>::with_capacity(cnt);
        let mut vvec = Vec::<parc::VWeak<u64>>::with_capacity(cnt);
        measure!("Parc:downgrade*".to_string(), cnt, {
            for _ in 0..cnt {
                pvec.push(Parc::downgrade(&b, j));
            }
        });
        measure!("Parc:upgrade^".to_string(), cnt, {
            for i in 0..cnt {
                pvec[i].upgrade(j).unwrap();
            }
        });
        measure!("Parc:demote*".to_string(), cnt, {
            for _ in 0..cnt {
                unsafe { vvec.push(Parc::unsafe_demote(&b)); }
            }
        });
        measure!("Parc:promote^".to_string(), cnt, {
            for i in 0..cnt {
                let _p = vvec[i].promote(j).unwrap();
            }
        });
    }).unwrap();

    for s in &sizes {
        let layout = std::alloc::Layout::from_size_align(*s * 8, 4).unwrap();
        measure!(format!("malloc({})", *s * 8), cnt, {
            for _ in 0..cnt {
                unsafe{ std::alloc::alloc(layout); }
            }
        });
    }

    let mut vec = Vec::with_capacity(cnt);
    measure!(" *Vec::push".to_string(), cnt, {
        for i in 0..cnt {
            vec.push(i as u128);
        }
    });
    let mut m = 0;
    measure!(" ^Vec::deref".to_string(), cnt, {
        for i in 0..cnt {
            m = vec[i];
        }
    });
    if m == cnt as u128 + 1 {
        println!("unreachable {}", m)
    }
    measure!(" for".to_string(), cnt, {
        for _ in 0..cnt {
            unsafe { asm!("nop"); }
        }
    });
    if m == cnt as u128 + 1 {
        println!("unreachable {}", m);
    }

    eprintln!("{}", report());
}
