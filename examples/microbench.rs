#![feature(asm)]

use corundum::stm::*;
use corundum::default::{*, Journal};
use corundum::stat::*;
use std::time::Instant;

type P = BuddyAlloc;

macro_rules! datalog {
    ($cnt:expr,$s:expr) => {
        P::transaction(|j| unsafe {
            let mut bvec = Vec::with_capacity($cnt);
            for _ in 0..$cnt {
                bvec.push(Pbox::new([0u8;$s], j));
            }
            measure!(format!("DataLog({})", $s), $cnt, {
                for i in 0..$cnt {
                    (&*bvec[i]).take_log(j, Notifier::None);
                }
            });
            j.ignore();
        }).unwrap();
    };
}

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

    struct Root {
        val: PRefCell<PVec<Option<Pbox<i32>>>>
    }

    impl RootObj<P> for Root {
        fn init(j: &Journal) -> Self {
            let mut v = PVec::with_capacity(100000, j);
            for _ in 0..100000 {
                v.push(None, j);
            }
            Self {
                val: PRefCell::new(v, j)
            }
        }
    }

    let root = P::open::<Root>(&args[1], O_CF | O_32GB).unwrap();
    for _ in 0..cnt {
        // Warm-up the allocator
        let s = 8 + rand::random::<usize>() % 5000;
        unsafe { P::alloc(s); }
    }
    measure!("TxNop".to_string(), cnt, {
        for _ in 0..cnt {
            P::transaction(|_| {unsafe { asm!("nop"); }}).unwrap();
        }
    });
    for s in &sizes {
        let s = *s * 8;
        let mut vec = Vec::with_capacity(cnt);
        measure!(format!("Alloc({})", s), cnt, {
            for _ in 0..cnt {
                unsafe{ vec.push(P::alloc(s)) }
            }
        });
        measure!(format!("Dealloc({})", s), cnt, {
            for i in 0..cnt {
                unsafe{ P::dealloc(vec[i].0, s); }
            }
        });
    }

    {
        let b = &*root.val.borrow();
        measure!("AtomicInit(8)".to_string(), cnt, {
            for i in 0..cnt {
                Pbox::initialize(&b[i], 10).unwrap();
            }
        });
    }

    for _ in 0 .. cnt/25 {
        let cnt = 25;
        P::transaction(|j| {
            let mut bvec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                bvec.push(Pbox::new(PRefCell::new(10, j), j));
            }
            let mut pvec = Vec::with_capacity(cnt);
            for i in 0..cnt {
                pvec.push(bvec[i].borrow_mut(j));
            }
            for i in 0..cnt {
                measure!("DerefMut(1st)".to_string(), cnt, {
                    *pvec[i] = 20;
                });
            }
        }).unwrap();
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

        datalog!(cnt, 8);
        datalog!(cnt, 64);
        datalog!(cnt, 256);
        datalog!(cnt, 1024);
        datalog!(cnt, 4096);
    
        for s in [8, 64, 2048, 8192, 32768].iter() {
            P::transaction(|j| unsafe {
                let mut vec = Vec::with_capacity(cnt);
                for _ in 0..cnt {
                    let m = P::alloc(*s);
                    if m.0.is_null() {
                        panic!("Could not alloc(2) mem size {}", s);
                    }
                    vec.push(m);
                }
                measure!(format!("DropLog({})", s), cnt, {
                    for i in 0..cnt {
                        let (_, off, len) = vec[i];
                        Log::drop_on_commit(off, len, j);
                    }
                });
            }).unwrap();
        }
    
        P::transaction(|j| {
            let b = Pbox::new(0u64, j);
            let mut vec = Vec::with_capacity(cnt);
            measure!("Pbox:clone".to_string(), cnt, {
                for _ in 0..cnt {
                    vec.push(b.pclone(j));
                }
            });
        }).unwrap();
    
        P::transaction(|j| {
            let b = Prc::new(0u64, j);
            let mut vec = Vec::with_capacity(cnt);
            measure!("Prc:clone".to_string(), cnt, {
                for _ in 0..cnt {
                    vec.push(b.pclone(j));
                }
            });
        }).unwrap();

        P::transaction(|j| {
            let b = Prc::new(0u64, j);
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(measure!("Prc:2clone".to_string(), {
                    b.pclone(j)
                }));
            }
        }).unwrap();

        P::transaction(|j| {
            let b = Parc::new(0u64, j);
            let mut vec = Vec::with_capacity(cnt);
            measure!("Parc:clone".to_string(), cnt, {
                for _ in 0..cnt {
                    vec.push(b.pclone(j));
                }
            });
        }).unwrap();
    }

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        let mut pvec = Vec::with_capacity(cnt);
        measure!("Prc:downgrade".to_string(), cnt, {
            for _ in 0..cnt {
                pvec.push(Prc::downgrade(&b, j));
            }
        });
        let mut bvec = Vec::with_capacity(cnt);
        measure!("Prc:upgrade".to_string(), cnt, {
            for i in 0..cnt {
                bvec.push(pvec[i].upgrade(j))
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Prc::new(0u64, j);
        let mut vvec = Vec::<prc::VWeak<u64>>::with_capacity(cnt);
        unsafe { 
            measure!("Prc:demote".to_string(), cnt, {
                for _ in 0..cnt {
                    vvec.push(Prc::unsafe_demote(&b));
                }
            })
        }
        let mut bvec = Vec::with_capacity(cnt);
        measure!("Prc:promote".to_string(), cnt, {
            for i in 0..cnt {
                bvec.push(vvec[i].promote(j));
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        let mut pvec = Vec::<parc::PWeak<u64>>::with_capacity(cnt);
        measure!("Parc:downgrade".to_string(), cnt, {
            for _ in 0..cnt {
                pvec.push(Parc::downgrade(&b, j));
            }
        });
        let mut ppvec = Vec::with_capacity(cnt);
        let _p = measure!("Parc:upgrade".to_string(), cnt, {
            for i in 0..cnt {
                ppvec.push(pvec[i].upgrade(j))
            }
        });
    }).unwrap();

    P::transaction(|j| {
        let b = Parc::new(0u64, j);
        let mut vvec = Vec::with_capacity(cnt);
        unsafe { 
            measure!("Parc:demote".to_string(), cnt, {
                for _ in 0..cnt {
                    vvec.push(Parc::unsafe_demote(&b));
                }
            })
        }
        let mut pvec = Vec::with_capacity(cnt);
        measure!("Parc:promote".to_string(), cnt, {
            for i in 0..cnt {
                pvec.push(vvec[i].promote(j));
            }
        })
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

    for _ in 0..cnt {
        measure!(" nop".to_string(), {
            unsafe { asm!("nop"); }
        });
    }

    println!("{}", report());
}
