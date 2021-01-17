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

    let root = P::open::<PRefCell<Option<Pbox<i32>>>>(&args[1], O_CF | O_32GB).unwrap();
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
            let m = measure!(format!("Alloc({})", s), {
                unsafe{ P::alloc(s) }
            });
            if m.0.is_null() {
                panic!("Could not alloc mem size {}", s);
            }
            vec.push(m); 
        }
        for i in 0..cnt {
            let off = vec[i].0;
            measure!(format!("Dealloc({})^", s), {
                unsafe{ P::dealloc(off, s); }
            });
        }
    }
    for _ in 0 .. cnt/50 {
        let cnt = 50;
        P::transaction(|j| {
            for _ in 0..cnt {
                let b = Pbox::new(PRefCell::new(10, j), j);
                let mut b = b.borrow_mut(j);
                measure!("DerefMut(1st)".to_string(), {
                    *b += 20;
                });
            }
        }).unwrap();
        for _ in 0..cnt {
            {
                let b = &*root.borrow();
                measure!("AtomicInit(8)".to_string(), {
                    Pbox::initialize(b, 10).unwrap();
                });
            }
            P::transaction(|j| {
                root.replace(None, j);
            }).unwrap();
        }
        P::transaction(|j| {
            let b = Pbox::new(10, j);
            let mut v = 0;
            measure!("Deref".to_string(), cnt, {
                for _ in 0..cnt {
                    v = *b;
                }
            });
            if v < 0 {
                println!("unreachable {}", v);
            }
        }).unwrap();
        P::transaction(|j| {
            let b = Pbox::new(Pbox::new(10, j), j);
            let mut b = &**b;
            let mut m = &mut b;
            measure!("DerefMut(!1st)".to_string(), cnt, {
                for _ in 0..cnt {
                    m = &mut b;
                }
            });
            if **m < 0 {
                println!("unreachable {}", m);
            }
        }).unwrap();
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
                let m = P::alloc(8);
                if m.0.is_null() {
                    panic!("Could not alloc(2) mem size 8");
                }
                vec.push(m);
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
                let m = P::alloc(64);
                if m.0.is_null() {
                    panic!("Could not alloc(3) mem size 64");
                }
                vec.push(m);
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
                let m = P::alloc(2048);
                if m.0.is_null() {
                    panic!("Could not alloc(4) mem size 2048");
                }
                vec.push(m);
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
                let len = 8*1024;
                let m = P::alloc(len);
                if m.0.is_null() {
                    panic!("Could not alloc(5) mem size {}", len);
                }
                vec.push(m);
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
                let len = 32*1024;
                let m = P::alloc(len);
                if m.0.is_null() {
                    panic!("Could not alloc(5) mem size {}", len);
                }
                vec.push(m);
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
                vec.push(measure!("Pbox:clone*".to_string(), {
                    b.pclone(j)
                }));
            }
        }).unwrap();
    
        P::transaction(|j| {
            let b = Prc::new(0u64, j);
            let mut vec = Vec::<Prc<u64>>::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(measure!("Prc:clone*".to_string(), {
                    b.pclone(j)
                }));
            }
        }).unwrap();
    
        P::transaction(|j| {
            let b = Parc::new(0u64, j);
            let mut vec = Vec::<Parc<u64>>::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(measure!("Parc:clone*".to_string(), {
                    b.pclone(j)
                }));
            }
        }).unwrap();
    }

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        let mut vec = Vec::<prc::PWeak<u64>>::with_capacity(cnt);
        for _ in 0..cnt {
            vec.push(measure!("Prc:downgrade*".to_string(), {
                Prc::downgrade(&b, j)
            }));
        }
        for i in 0..cnt {
            let p = &vec[i];
            let _p = measure!("Prc:upgrade^".to_string(), {
                p.upgrade(j)
            }).unwrap();
        }
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        let mut pvec = Vec::<parc::PWeak<u64>>::with_capacity(cnt);
        let mut vvec = Vec::<parc::VWeak<u64>>::with_capacity(cnt);
        for _ in 0..cnt {
            pvec.push(measure!("Parc:downgrade*".to_string(), {
                Parc::downgrade(&b, j)
            }));
        }
        for i in 0..cnt {
            let p = &pvec[i];
            let _p = measure!("Parc:upgrade^".to_string(), {
                p.upgrade(j)
            }).unwrap();
        }
        for _ in 0..cnt {
            unsafe { 
                vvec.push(measure!("Parc:demote*".to_string(), {
                    Parc::unsafe_demote(&b)
                }))
            }
        }
        for i in 0..cnt {
            let p = &vvec[i];
            let _p = measure!("Parc:promote^".to_string(), {
                p.promote(j)
            }).unwrap();
        }
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
