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

    let sizes = vec![512, 128, 32, 8, 1];
    let cnt = args[2].parse::<usize>().expect("Expected a number");

    let _pool = P::open_no_root(&args[1], O_CF | O_32GB).unwrap();
    for _ in 0..cnt {
        // Warm-up the allocator
        let s = 8 + rand::random::<usize>() % 5000;
        unsafe { P::alloc(s); }
    }
    for _ in 0..cnt {
        measure!("TxNop".to_string(), {
            P::transaction(|_| {unsafe { asm!("nop"); }}).unwrap();
        });
    }
    for s in &sizes {
        let s = *s * 8;
        let mut vec = Vec::with_capacity(cnt);
        for _ in 0..cnt {
            vec.push({
                measure!(format!("Alloc({})", s), {
                    unsafe{ P::alloc(s) }
                })
            }); 
        }
        for i in 0..cnt {
            let off = vec[i].0;
            measure!(format!("Dealloc({})^", s), {
                unsafe{ P::dealloc(off, s); }
            });
        }
    }

    P::transaction(|j| {
        let b = Pbox::new(10, j);
        let mut v = 0;
        measure!("Deref".to_string(), cnt, {
            for _ in 0..cnt {
                v += *b;
            }
        });
        for _ in 0..cnt {
            let mut b = Pbox::new(10, j);
            measure!("DerefMut".to_string(), {
                *b += 20;
            });
        }
        if v < 0 {
            println!("unreachable {}", v);
        }
    }).unwrap();

    for _ in 0 .. cnt/40 {
        let cnt = 40;
        P::transaction(|j| unsafe {
            let b = Pbox::new(0u64, j);
            for _ in 0..cnt {
                let b = &*b;
                measure!("DataLog(8)".to_string(), {
                    b.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let b = Pbox::new([0u64;8], j);
            for _ in 0..cnt {
                let b = &*b;
                measure!("DataLog(64)".to_string(), {
                    b.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let b = Pbox::new([0u64;32], j);
            for _ in 0..cnt {
                let b = &*b;
                measure!("DataLog(2K)".to_string(), {
                    b.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let b = Pbox::new([0u64;128], j);
            for _ in 0..cnt {
                let b = &*b;
                measure!("DataLog(8K)".to_string(), {
                    b.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let b = Pbox::new([0u64;512], j);
            for _ in 0..cnt {
                let b = &*b;
                measure!("DataLog(32K)".to_string(), {
                    b.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(P::alloc(8));
            }
            for i in 0..cnt {
                let (_, off, len) = vec[i];
                measure!("DropLog(8)".to_string(), {
                    Log::drop_on_commit(off, len, j);
                });
            }
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(P::alloc(64));
            }
            for i in 0..cnt {
                let (_, off, len) = vec[i];
                measure!("DropLog(64)".to_string(), {
                    Log::drop_on_commit(off, len, j);
                });
            }
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(P::alloc(2048));
            }
            for i in 0..cnt {
                let (_, off, len) = vec[i];
                measure!("DropLog(2K)".to_string(), {
                    Log::drop_on_commit(off, len, j);
                });
            }
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(P::alloc(8*1024));
            }
            for i in 0..cnt {
                let (_, off, len) = vec[i];
                measure!("DropLog(8K)".to_string(), {
                    Log::drop_on_commit(off, len, j);
                });
            }
        }).unwrap();
    
        P::transaction(|j| unsafe {
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(P::alloc(32*1024));
            }
            for i in 0..cnt {
                let (_, off, len) = vec[i];
                measure!("DropLog(32K)".to_string(), {
                    Log::drop_on_commit(off, len, j);
                });
            }
        }).unwrap();
    
        P::transaction(|j| {
            let b = Pbox::new(0u64, j);
            let mut vec = Vec::<Pbox<u64>>::with_capacity(cnt);
            for _ in 0..cnt {
                measure!("Pbox:clone*".to_string(), {
                    vec.push(b.pclone(j));
                });
            }
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
    }

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
