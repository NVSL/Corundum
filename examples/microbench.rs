#![feature(asm)]

use corundum::stm::*;
use corundum::default::{*, Journal};
use corundum::stat::*;

type P = BuddyAlloc;
const CNT: usize = 50000;

macro_rules! datalog {
    ($cnt:expr,$s:expr) => {
        P::transaction(|j| unsafe {
            let mut bvec = Vec::with_capacity($cnt);
            for _ in 0..$cnt {
                bvec.push(Pbox::new([0u8;$s], j));
            }
            for i in 0..$cnt {
                let m = &*bvec[i];
                measure!(format!("DataLog({})", $s), {
                    m.take_log(j, Notifier::None);
                });
            }
            j.ignore();
        }).unwrap();
    };
}

fn main() {
    use std::env;
    use std::vec::Vec as StdVec;

    let args: StdVec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} file-name", args[0]);
        return;
    }

    let sizes = vec![512, 128, 32, 8, 1];

    struct Root {
        bx: PRefCell<PVec<Option<Pbox<i32>>>>,
        rc: PRefCell<PVec<Option<Prc<i32>>>>,
        arc: PRefCell<PVec<Option<Parc<i32>>>>,
    }

    impl RootObj<P> for Root {
        fn init(j: &Journal) -> Self {
            let mut b = PVec::with_capacity(CNT, j);
            for _ in 0..CNT {
                b.push(None, j);
            }
            let mut r = PVec::with_capacity(CNT, j);
            for _ in 0..CNT {
                r.push(None, j);
            }
            let mut a = PVec::with_capacity(CNT, j);
            for _ in 0..CNT {
                a.push(None, j);
            }
            Self {
                bx: PRefCell::new(b),
                rc: PRefCell::new(r),
                arc: PRefCell::new(a),
            }
        }
    }

    let root = P::open::<Root>(&args[1], O_CF | O_32GB).unwrap();
    let cnt = CNT;
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
            vec.push(measure!(format!("Alloc({})", s), {
                unsafe{ P::alloc(s) }
            }));
        }
        for i in 0..cnt {
            measure!(format!("Dealloc({})", s), {
                unsafe{ P::dealloc(vec[i].0, s); }
            });
        }
    }

    {
        let b = &*root.bx.borrow();
        for i in 0..cnt {
            let b = &b[i];
            measure!("Pbox:AtomicInit".to_string(), {
                Pbox::initialize(b, 10)
            }).unwrap();
        }
        let r = &*root.rc.borrow();
        for i in 0..cnt {
            let r = &r[i];
            measure!("Prc:AtomicInit".to_string(), {
                Prc::initialize(r, 10)
            }).unwrap();
        }
        let a = &*root.arc.borrow();
        for i in 0..cnt {
            let a = &a[i];
            measure!("Parc:AtomicInit".to_string(), {
                Parc::initialize(a, 10)
            }).unwrap();
        }
    }

    for _ in 0 .. CNT/50 {
        let cnt = 50;
        P::transaction(|j| {
            let mut bvec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                bvec.push(Pbox::new(PRefCell::new(10), j));
            }
            let mut pvec = Vec::with_capacity(cnt);
            for i in 0..cnt {
                pvec.push(bvec[i].borrow_mut(j));
            }
            for i in 0..cnt {
                measure!("DerefMut(1st)".to_string(), {
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
                for i in 0..cnt {
                    let (_, off, len) = vec[i];
                    measure!(format!("DropLog({})", s), {
                        Log::drop_on_commit(off, len, j);
                    });
                }
            }).unwrap();
        }
    
        P::transaction(|j| {
            let b = Pbox::new(0u64, j);
            let mut vec = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                vec.push(measure!("Pbox:clone".to_string(), {
                    b.pclone(j)
                }));
            }
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
