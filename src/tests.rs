#[cfg(test)]
pub(crate) mod problems {
    use crate::default::*;
    use crate::boxed::Pbox;
    use crate::cell::*;
    use crate::stm::*;
    use crate::stm::Journal;

    #[test]
    #[ignore]
    fn challenge_mt() {
        use crate::sync::*;

        type P = BuddyAlloc;

        struct Root {
            v1: Parc<PMutex<i32, P>, P>,
        }

        impl RootObj<P> for Root {
            fn init(j: &Journal<P>) -> Self {
                Root {
                    v1: Parc::new(PMutex::new(0), j),
                }
            }
        }

        std::thread::spawn(|| {
            let root = P::open::<Root>("challenge_mt1.pool", O_CFNE).unwrap();
            let v1 = root.v1.demote();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(1));
                P::transaction(|j| {
                    if let Some(r) = v1.promote(j) {
                        let mut r = r.lock(j);
                        *r += 1;
                        println!("root = {}", *r);
                    }
                })
                .unwrap();
            });
        });
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    #[test]
    fn test_vec_cons() {
        use crate::str::*;
        use crate::sync::*;
        use crate::vec::Vec;
        use std::thread;

        type P = BuddyAlloc;

        struct Root {
            v1: Parc<PMutex<Vec<String<P>, P>, P>, P>,
        }

        impl RootObj<P> for Root {
            fn init(j: &Journal<P>) -> Self {
                Root {
                    v1: Parc::new(PMutex::new(Vec::new()), j),
                }
            }
        }

        let root = P::open::<Root>("test_vec_cons.pool", O_CFNE).unwrap();

        let mut threads = vec![];
        P::transaction(|j| {
            let mut v = root.v1.lock(j);
            v.clear();
        })
        .unwrap();
        for i in 0..1 {
            let v1 = root.v1.demote();
            threads.push(thread::spawn(move || {
                P::transaction(move |j| {
                    if let Some(v1) = v1.promote(j) {
                        let mut v = v1.lock(j);
                        println!("{:?} old value = {:#?}", thread::current().id(), v);
                        v.push(
                            format!("{:?} pushed {}", thread::current().id(), i).to_pstring(j),
                            j,
                        );
                        println!("{:?} new value = {:#?}", thread::current().id(), v);
                        if i == 6 {
                            // std::process::exit(0);
                        }
                    }
                })
                .unwrap();
            }));
        }

        for thread in threads {
            let _ = thread.join().unwrap();
        }
        println!("Memory usage = {}", P::used());
    }

    #[test]
    fn paper() {
        use crate::default::*;
        type P = BuddyAlloc;
        struct Node { val: i32, next: PRefCell<Option<Pbox<Node>>> }
        impl RootObj<P> for Node {
            fn init(_j: &Journal) -> Self { Self{
                val: 0, next: PRefCell::new(None)
            }}
        }
        fn append(n: &Node, v:i32, j: &Journal) {
            let mut t = n.next.borrow_mut(j);
            match &*t {
                Some(succ) => append(succ, v, j),
                None => *t = Some(Pbox::new(
                    Node {
                    val: v,
                    next: PRefCell::new(None)
                    }, j))
            }
        }
        fn go(v: i32) {
            let head = BuddyAlloc::open::<Node>("list.pool",O_CFNE).unwrap();
            transaction(|j| {
                append(&head, v, j);
            }).unwrap();
        }

        fn print(n: &Node) {
            let t = n.next.borrow();
            print!("{} ", n.val);
            match &*t {
                Some(succ) => print(succ),
                None => {}
            }
        }

        fn print_all() {
            let head = BuddyAlloc::open::<Node>("list.pool",O_CFNE).unwrap();
            print(&head);
            println!();
        }

        go(rand::random());
        print_all();
    }

    #[test]
    #[ignore]
    fn memory_leak_problem() {
        use crate::default::*;
        use std::time::Duration;

        type P = BuddyAlloc;

        let _pool = P::open_no_root("my_test.pool", O_CF).unwrap();

        let _ = P::transaction(|j| {
            let a = Parc::new(42, j);
            let b = a.demote();
            std::thread::spawn(move || {
                let _ = P::transaction(|j| {
                    std::thread::sleep(Duration::from_millis(900));
                    if let Some(b) = b.promote(j) {
                        println!("Exit {}", *b);
                        std::process::exit(0); // Memory leak may happen here
                    }
                })
                .unwrap();
            });
            std::thread::sleep(Duration::from_millis(600));
            println!("{}", *a);
        });
        std::thread::sleep(Duration::from_millis(1000));
        crate::tests::test::print_usage(0);
    }

    #[test]
    #[ignore]
    fn test_pack1() {
        use crate::alloc::*;
        use std::time::Duration;

        type P = BuddyAlloc;

        struct Root {
            data: Parc<PMutex<Parc<u32>>>,
        }

        impl RootObj<P> for Root {
            fn init(j: &Journal<P>) -> Self {
                Root {
                    data: Parc::new(PMutex::new(Parc::new(10, j)), j),
                }
            }
        }

        let root = P::open::<Root>("test_pack.pool", O_CFNE).unwrap();
        crate::tests::test::print_usage(0);

        let b = root.data.demote();
        std::thread::spawn(move || {
            P::transaction(|j| {
                std::thread::sleep(Duration::from_millis(900));
                if let Some(b) = b.promote(j) {
                    let mut b = b.lock(j);
                    *b = Parc::new(**b + 1, j);
                    println!("data1 {}", *b);
                    // std::process::exit(0); // Memory leak may happen here
                }
            })
            .unwrap();
        });
        std::thread::sleep(Duration::from_millis(1000));
        crate::tests::test::print_usage(1);

        let b = root.data.demote();
        P::transaction(|_| {
            std::thread::spawn(move || {
                P::transaction(|j| {
                    std::thread::sleep(Duration::from_millis(900));
                    if let Some(b) = b.promote(j) {
                        let mut b = b.lock(j);
                        *b = Parc::new(**b + 1, j);
                        println!("data2 {}", *b);
                        // std::process::exit(0); // Memory leak may happen here
                    }
                })
                .unwrap();
            });
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(1000));
        crate::tests::test::print_usage(2);
    }

    #[test]
    #[should_panic]
    fn test_pack2() {
        use crate::alloc::*;
        use std::time::Duration;

        type P = BuddyAlloc;

        let _img = P::open_no_root("nosb.pool", O_CF).unwrap();
        crate::tests::test::print_usage(0);

        P::transaction(|j| {
            let root = Parc::new(10, j);
            let b = root.demote(); // Panics here because `pack` should be called
                                 // outside transaction
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(900));
                P::transaction(|j| {
                    if let Some(b) = b.promote(j) {
                        println!("data1 {}", *b);
                        std::thread::sleep(Duration::from_millis(2000));
                        std::process::exit(0); // Memory leak may happen here
                    }
                }).unwrap();
            });
            std::thread::sleep(Duration::from_millis(1000));
        }).unwrap();
        crate::tests::test::print_usage(1);
    }

    #[test]
    fn ref_cycle_mem_leak() {
        use crate::default::*;
        type P = BuddyAlloc;

        #[derive(Debug)]
        enum List {
            Cons(i32, PRefCell<Prc<List>>),
            Nil,
        }

        use List::*;

        impl List {
            fn tail(&self) -> Option<&PRefCell<Prc<List>>> {
                match self {
                    Cons(_, item) => Some(item),
                    Nil => None,
                }
            }
        }

        let _img = P::open_no_root("ref_cycle_mem_leak.pool", O_CF).unwrap();
        println!("usage 1: {}", P::used());
        P::transaction(|j| {
            let a = Prc::new(Cons(5, PRefCell::new(Prc::new(Nil, j))), j);

            println!("a initial rc count = {}", Prc::strong_count(&a));
            println!("a next item = {:?}", a.tail());
        
            let b = Prc::new(Cons(10, PRefCell::new(Prc::pclone(&a, j))), j);
        
            println!("a rc count after b creation = {}", Prc::strong_count(&a));
            println!("b initial rc count = {}", Prc::strong_count(&b));
            println!("b next item = {:?}", b.tail());
        
            if let Some(link) = a.tail() {
                *link.borrow_mut(j) = Prc::pclone(&b, j);
            }
        
            println!("b rc count after changing a = {}", Prc::strong_count(&b));
            println!("a rc count after changing a = {}", Prc::strong_count(&a));
        }).unwrap();
        println!("usage 2: {}", P::used());
    }

    #[test]
    fn test_vweak() {
        use crate::default::*;
        type P = BuddyAlloc;

        struct Root {
            v: PRefCell<Prc<u32>>,
        }

        impl RootObj<P> for Root {
            fn init(j: &Journal) -> Self {
                Root {
                    v: PRefCell::new(Prc::new(10, j)),
                }
            }
        }

        let ovp = {
            let root = P::open::<Root>("test_vweak.pool", O_CFNE).unwrap();
            let vp = Prc::demote(&root.v.borrow());
            P::transaction(|j| {
                if let Some(p) = vp.promote(j) {

                    let _vp2 = Prc::demote(&p);
                    // drop a recently created volatile reference
                    let _vp2 = Prc::demote(&p);
                    // drop a recently created volatile reference
                    let _vp2 = Prc::demote(&p);
                    // drop a recently created volatile reference
                    let _vp2 = Prc::demote(&p);
                // drop a recently created volatile reference
                } else {
                    println!("no data");
                }
                
                // Dropping the old Prc
                let mut b = root.v.borrow_mut(j);
                *b = Prc::new(12, j);

                if let Some(p) = vp.promote(j) {
                    println!("new data = {}", p);
                } else {
                    println!("no new data");
                }
            })
            .unwrap();

            let x = Prc::demote(&root.v.borrow());
            P::transaction(|j| {
                // Trying to access a volatile pointer from the current session
                if let Some(p) = x.promote(j) {
                    println!("data = {}", p);
                } else {
                    println!("no data");
                }
            })
            .unwrap();
            x
        };
        println!("pool is closed");
        // Reopening the pool
        let _root = P::open::<Root>("test_vweak.pool", O_CFNE).unwrap();
        P::transaction(|j| {
            // Trying to access a volatile pointer from previous session
            if let Some(p) = ovp.promote(j) {
                println!("data = {}", p);
            } else {
                println!("no data");
            }
        })
        .unwrap();
    }

    #[test]
    fn trans_inside_fn() {
        use crate::cell::PRefCell as RefCell;
        use crate::boxed::Pbox;

        crate::pool!(pool1);
        crate::pool!(pool2);

        type P1 = pool1::BuddyAlloc;
        type P2 = pool2::BuddyAlloc;

        fn foo<M: MemPool>(root: &RefCell<i32, M>, v: i32) -> i32 {
            M::transaction(|j| {
                let mut root = root.borrow_mut(j);
                *root = v;
                *root
            }).unwrap()
        }

        let root1 = P1::open::<Pbox<RefCell<i32, P1>, P1>>("pool5.pool", O_CFNE).unwrap();
        let root2 = P2::open::<Pbox<RefCell<i32, P2>, P2>>("pool6.pool", O_CFNE).unwrap();
        let _res = Chaperon::session("foo1.pool", || {
            let other = foo::<P2>(&root2, 10); // <-- foo commits here
            P1::transaction(|j| {
                let mut root = root1.borrow_mut(j);
                *root = other
            }).unwrap();
            P2::transaction(|j| {
                // <-- Creates a Journal<P2>
                let mut root = root2.borrow_mut(j);
                *root = other;
                // std::process::exit(0);
            }).unwrap();
            let _other = foo::<P1>(&root1, 20); // <-- foo dose not commit here (postponed) because a trans is open
        });

        println!("P1: {}", P1::used());
        println!("P2: {}", P2::used());
    }

    #[test]
    #[cfg(feature = "refcell_lifetime_change")]
    fn test_refcell_ownership() {
        use crate::default::*;

        type P = BuddyAlloc;

        let root = P::open::<Prc<PRefCell<i32>>>("test.pool", O_CF).unwrap();

        P::transaction(|j| {
            let mut a = root.borrow_mut(j);
            *a = 10;
            let b = PRefMut::into_inner(a);
            let mut c = b.borrow_mut(j);
            *c = 20;
            let mut d = PRefMut::own(c);
            *d = 25;
            // let e = root.borrow_mut(j); <-- Error: multiple mutable borrows
            // let f = root.borrow(); <-- Error: already mutably borrowed

            let new = Pbox::new(PRefCell::pfrom(d, j), j); // A new `PRefCell`
            let mut e = new.borrow_mut(j);
            *e = 30; // The original PRefCell won't change

            // `d` is still available here 
        }).unwrap();

        assert_eq!(25, *root.borrow());
    }

    #[test]
    fn test_concurrent() {
        use crate::alloc::*;
        use std::time::Instant;

        let _pool = BuddyAlloc::open_no_root("conc.pool", O_CF).unwrap();

        let mut threads = vec![];
        let start = Instant::now();
        for _ in 0..4 {
            threads.push(std::thread::spawn(move || {
                BuddyAlloc::transaction(|j| {
                    let mut v = vec![];
                    for _ in 0..100 {
                        v.push(Pbox::new(0, j));
                    }
                })
                .unwrap();
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        let duration = start.elapsed();
        println!("Time elapsed in parallel execution is: {:?}", duration);
        println!("usage = {} bytes", BuddyAlloc::used());

        let start = Instant::now();
        for _ in 0..4 {
            BuddyAlloc::transaction(|j| {
                let mut v = vec![];
                for _ in 0..100 {
                    v.push(Pbox::new(0, j));
                }
            })
            .unwrap();
        }
        let duration = start.elapsed();
        println!("Time elapsed in serial execution is: {:?}", duration);
        println!("usage = {} bytes", BuddyAlloc::used());
    }

    #[test]
    fn parc_two_threads() {
        use crate::default::*;

        type P = BuddyAlloc;
        let root = P::open::<Parc<i32>>("parc_two_threads.pool", O_CFNE).unwrap();
        println!("usage: {}", P::used());
        println!("strong = {}", Parc::strong_count(&root));
        let c1 = root.demote();
        let c2 = root.demote();
        let t1 = std::thread::spawn(move || {
            let _=P::transaction(|j| {
                if let Some(p) = c2.promote(j) {
                    let m = p.pclone(j);
                    println!("strong(t1) = {}", Parc::strong_count(&m));
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    println!("t1 is done");
                }
            });
        });
        
        let t2 = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(25));
            let _ = P::transaction(|j| {
                if let Some(p) = c1.promote(j) {
                    let m = p.pclone(j);
                    println!("strong(t2) = {}", Parc::strong_count(&m));
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    // std::process::exit(0);
                    panic!("abort");
                }
            });
        });

        t1.join().unwrap();
        t2.join().unwrap();
        assert_eq!(Parc::strong_count(&root), 1);
    }
}

#[cfg(test)]
pub(crate) mod test {
    use crate::alloc::heap::Heap;
    use crate::default::*;
    use crate::cell::*;
    use crate::prc::*;
    use crate::stm::*;
    use crate::stm::Journal;
    use crate::sync::PMutex;
    use crate::*;

    type A = BuddyAlloc;

    #[test]
    #[ignore]
    fn test_mutex_deadlock() {
        // Race condition problem (cyclic locks) is still possible

        struct Root {
            a: PMutex<u32, A>,
            b: PMutex<u32, A>,
        }

        impl RootObj<A> for Root {
            fn init(_j: &Journal<A>) -> Self {
                Self {
                    a: PMutex::new(0),
                    b: PMutex::new(0),
                }
            }
        }

        let root = A::open::<Root>("test_mutex_deadlock.pool", O_CFNE).unwrap();

        let t1 = {
            let root = root.clone();
            thread::spawn(move || {
                A::transaction(|j| {
                    let mut a = root.a.lock(j);
                    *a += 1;
                    let mut b = root.b.lock(j);
                    *b += 1;
                })
                .unwrap()
            })
        };

        let t2 = {
            let root = root.clone();
            thread::spawn(move || {
                A::transaction(|j| {
                    let mut b = root.b.lock(j);
                    *b += 1;
                    let mut a = root.a.lock(j);
                    *a += 1;
                })
                .unwrap()
            })
        };

        print_usage(0);

        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[test]
    // #[ignore]
    fn test_string_mt() {
        use crate::str::String;

        struct ConsumerData {
            buf: String<A>,
        }
        pub struct Consumer {
            pattern: String<A>,
            data: PMutex<String<A>, A>,
        }

        impl RootObj<A> for Consumer {
            fn init(_j: &Journal<A>) -> Self {
                Self {
                    pattern: String::new(),
                    data: PMutex::new(String::new()),
                }
            }
        }

        let root = A::open::<Parc<Consumer, A>>("consumer.pool", O_CFNE).unwrap();

        let mut threads = vec![];
        for i in 0..10 {
            let root = root.demote();
            threads.push(thread::spawn(move || {
                A::transaction(|j| {
                    if let Some(data) = root.promote(j) {
                        let mut data = data.data.lock(j);
                        let r = rand().to_string();
                        println!("Old persisted data: `{}`", data);
                        println!("New data to be persisted: `{}`", r);
                        *data = String::from_str(&r, j);
                        if i == 6 {
                            // std::process::exit(0);
                        }
                    }
                })
                .unwrap()
            }))
        }

        for thread in threads {
            thread.join().unwrap()
        }
    }

    #[test]
    fn multiple_transactions() {
        let _image = A::open_no_root("nosb.pool", O_CF).unwrap();

        A::transaction(|j1| {
            let b1 = Pbox::new(default::PCell::new(1), j1);
            b1.set(
                Heap::transaction(move |j2| {
                    let b2 = Pbox::new(heap::PCell::new(1), j2);
                    b2.get()
                })
                .unwrap(),
                j1,
            );
        })
        .unwrap();
    }

    #[test]
    #[ignore]
    fn multiple_open() {
        let mut threads = vec![];

        for _ in 0..10 {
            threads.push(std::thread::spawn(move || {
                let _image = A::open_no_root("nosb.pool", O_CF).unwrap();
                A::transaction(|j1| {
                    let b1 = Pbox::new(default::PRefCell::new(1), j1);
                    let mut b1 = b1.borrow_mut(j1);
                    *b1 = Heap::transaction(move |j2| {
                        let b2 = Pbox::new(heap::PRefCell::new(1), j2);
                        b2.read()
                    })
                    .unwrap();
                })
                .unwrap();
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn test_maybe_null_drop() {
        struct D {
            val: i32,
        }
        impl Drop for D {
            fn drop(&mut self) {
                println!("Data {} dropped", self.val);
            }
        }
        struct MyData {
            data: [Option<D>; 8],
        }
        let mut my_data = MyData {
            data: Default::default(),
        };
        my_data.data[1] = Some(D { val: 12 });
    }

    #[test]
    fn multiple_pools() {
        use crate::cell::PRefCell;

        crate::pool!(pool1);
        crate::pool!(pool2);

        type P1 = pool1::BuddyAlloc;
        type P2 = pool2::BuddyAlloc;

        struct Root<P: MemPool> {
            val: Pbox<PRefCell<i32, P>, P>,
        }
        impl<M: MemPool> RootObj<M> for Root<M> {
            fn init(j: &Journal<M>) -> Self {
                Root {
                    val: Pbox::new(PRefCell::new(0), j),
                }
            }
        }

        let root1 = P1::open::<Root<P1>>("pool3.pool", O_CFNE).unwrap();
        let root2 = P2::open::<Root<P2>>("pool4.pool", O_CFNE).unwrap();

        let _ = Chaperon::session("chaperon2.pool", || {
            let v = P2::transaction(|j| {
                let mut p2 = root2.val.borrow_mut(j);
                let old = *p2;
                *p2 += 1; // <-- should persist if both transactions commit
                          // panic!("test");
                          // std::process::exit(0);
                old // <-- Send out p2's old data
            })
            .unwrap();
            // panic!("test");
            // std::process::exit(0);
            P1::transaction(|j| {
                let mut p1 = root1.val.borrow_mut(j);
                *p1 += v;
                // panic!("test");
                // std::process::exit(0);
            })
            .unwrap();
            // panic!("test");
            // std::process::exit(0);
        });

        let v1 = root1.val.read();
        let v2 = root2.val.read();
        println!("root1 = {} (usage = {})", v1, P1::used());
        println!("root2 = {} (usage = {})", v2, P2::used());
        assert_eq!(v1, calc(v2 - 1));

        fn calc(n: i32) -> i32 {
            if n < 1 {
                0
            } else {
                n + calc(n - 1)
            }
        }
    }

    #[test]
    fn concat_test() {
        use crate::default::*;

        type P = BuddyAlloc;
        type Ptr = Option<Prc<PRefCell<Node>>>;
        struct Node {
            val: i32,
            next: Ptr,
        }
        fn remove_all(k: i32) {
            let root = P::open::<Pbox<PRefCell<Ptr>>>("foo3.pool", 0).unwrap();
            P::transaction(|j| {
                let mut curr = root.borrow().pclone(j);
                let mut prev = prc::PWeak::<PRefCell<Node>>::new();
                while let Some(n) = curr {
                    let p = n.borrow();
                    if p.val == k {
                        if let Some(pr) = prev.upgrade(j) {
                            let mut pr = pr.borrow_mut(j);
                            pr.next = p.next.pclone(j);
                        } else {
                            *root.borrow_mut(j) = p.next.pclone(j);
                        }
                    }
                    prev = Prc::downgrade(&n, j);
                    curr = p.next.pclone(j);
                }
            })
            .expect("Unsuccessful");
        }
    }

    #[test]
    fn prc_test() {
        use crate::default::*;

        let _image = A::open_no_root("nosb.pool", O_CF).unwrap();

        struct Cell {
            k: i32,
            next: Option<Prc<PRefCell<Cell>>>,
        }

        impl Cell {
            pub fn new(x: i32) -> Self {
                Cell { k: x, next: None }
            }
            pub fn add(&mut self, v: i32, j: &Journal) {
                if let Some(next) = &self.next {
                    let mut next = next.borrow_mut(j);
                    next.add(v, j);
                } else {
                    self.next = Some(Prc::new(PRefCell::new(Cell::new(v)), j));
                }
            }
            pub fn sum(&self, j: &Journal) -> i32 {
                if let Some(next) = &self.next {
                    self.k + next.borrow().sum(j)
                } else {
                    self.k
                }
            }
        }

        transaction(|j| {
            let shared_root: Prc<PRefCell<Cell>> =
                Prc::new(PRefCell::new(Cell::new(10)), j);
            // Create a new block to limit the scope of the dynamic borrow
            {
                let mut root = shared_root.borrow_mut(j);
                root.add(20, j);
                root.add(30, j);
                root.add(40, j);
                root.add(50, j);
            }
            // Note that if we had not let the previous borrow of the cache fall out
            // of scope then the subsequent borrow would cause a dynamic thread panic.
            // This is the major hazard of using `RefCell`.
            let total: i32 = shared_root.borrow().sum(j);
            println!("{}", total);
        })
        .ok();

        // std::thread::spawn(move || {
        //     let total: i32 = shared_root.sum();
        //     println!("{}", total);
        // }).join().unwrap();
    }

    #[test]
    fn test_nv_to_v() {
        use std::cell::RefCell;
        use crate::default::*;
        use crate::stm::Journal;

        let root = A::open::<Cell>("nv2v.pool", O_CFNE).unwrap();

        struct Cell {
            k: VCell<RefCell<Option<Box<i32>>>>,
            next: PRefCell<Option<Prc<Cell>>>,
        }

        impl Default for Cell {
            fn default() -> Self {
                Cell::new(0)
            }
        }

        impl Cell {
            pub fn new(x: i32) -> Self {
                Cell {
                    k: VCell::new(RefCell::new(Some(Box::new(x)))),
                    next: PRefCell::new(None),
                }
            }
            pub fn add(&self, v: i32, j: &Journal<A>) {
                if let Some(next) = &*self.next.borrow() {
                    next.add(v, j);
                    return;
                }
                *self.next.borrow_mut(j) = Some(Prc::new(Cell::new(v), j));
            }
            pub fn sum(&self, j: &Journal<A>) -> i32 {
                let mut k = if let Some(g) = &*self.k.borrow() {
                    **g
                } else {
                    -1
                };
                if k == -1 {
                    println!("create a new pbox");
                    *self.k.borrow_mut() = Some(Box::new(0));
                    // self.k.replace(Some(Pbox::new(0, j)));
                    k = 0;
                };
                if let Some(next) = &*self.next.borrow() {
                    k + next.sum(j)
                } else {
                    k
                }
            }
        }

        let root = std::panic::AssertUnwindSafe(root);
        transaction(|j| {
            root.add(20, j);
            // root.add(30, j);
            // root.add(40, j);
            // root.add(50, j);
            // Note that if we had not let the previous borrow of the cache fall out
            // of scope then the subsequent borrow would cause a dynamic thread panic.
            // This is the major hazard of using `RefCell`.
            let total: i32 = root.sum(j);
            println!("{}", total);
        })
        .ok();

        print_usage(0);
        // std::thread::spawn(move || {
        //     let total: i32 = shared_root.sum();
        //     println!("{}", total);
        // }).join().unwrap();
    }

    pub fn print_usage(idx: i32) -> usize {
        let usage = A::used();
        println!("{:>4}: {:>8} bytes used", idx, usage);
        usage
    }

    #[test]
    fn test_sort_insert_volatile() {
        use std::rc::*;
        use std::cell::*;
        struct Node {
          val: i32,
          next: Rc<RefCell<Option<Node>>>
        }
        impl Node {
            fn insert(&self, val: i32) {
                let mut next = self.next.borrow_mut();
                if let Some(next) = &mut *next {
                    if next.val > val {
                        *next = Node {
                            val,
                            next: self.next.clone()
                        }
                    } else {
                        next.insert(val);
                    }
                } else {
                    *next = Some(Node {
                        val,
                        next: Rc::new(RefCell::new(None))
                    })
                }
            }
        }
    }

    fn test_sort_insert_pmem() {
        use crate::default::*;
        struct Node {
          val: i32,
          next: Prc<PRefCell<Option<Node>>>
        }
        impl Node {
            fn insert(&self, val: i32) {
                transaction(|j|{
                    let mut next = self.next.borrow_mut(j);
                    if let Some(n) = &*next {
                        if n.val > val {
                            *next = Some(Node {
                                val,
                                next: self.next.pclone(j)
                            })
                        } else {
                            n.insert(val);
                        }
                    } else {
                        *next = Some(Node {
                            val,
                            next: Prc::new(PRefCell::new(None), j)
                        })
                    }
                }).unwrap()
            }
        }
    }

    #[test]
    fn test_gadget_owners() {
        use crate::default::*;

        let _image = A::open_no_root("nosb.pool", O_CF);
        struct Owner {
            name: [u8; 8],
            gadgets: [Weak<Gadget, A>; 2],
        }
        struct Gadget {
            id: i32,
            owner: Prc<PRefCell<Owner>>,
        }
        impl Owner {
            fn new(name: &str) -> Self {
                let vec: Vec<u8> = name.as_bytes().to_vec();
                let mut name: [u8; 8] = [0; 8];
                for i in 0..8 {
                    name[i] = if i < vec.len() { vec[i] } else { 0 };
                }
                Owner {
                    name,
                    gadgets: [Weak::<Gadget, A>::new(), Weak::<Gadget, A>::new()],
                }
            }
            fn get_name(&self) -> String {
                let mut vec = Vec::<u8>::new();
                vec.extend_from_slice(&self.name);
                String::from_utf8(vec).unwrap()
            }
        }
        print_usage(0);
        transaction(|j| {
            // Create a reference-counted `Owner`. Note that we've put the `Owner`'s
            // vector of `Gadget`s inside a `RefCell` so that we can mutate it through
            // a shared reference.
            let gadget_owner: Prc<PRefCell<Owner>> =
                Prc::new(PRefCell::new(Owner::new("Dany")), j);
            print_usage(1);

            // Create `Gadget`s belonging to `gadget_owner`, as before.
            let gadget1 = Prc::new(
                Gadget {
                    id: 1,
                    owner: gadget_owner.pclone(j),
                },
                j,
            );
            let gadget2 = Prc::new(
                Gadget {
                    id: 2,
                    owner: gadget_owner.pclone(j),
                },
                j,
            );

            // Add the `Gadget`s to their `Owner`.
            {
                let mut gadget_owner = gadget_owner.borrow_mut(j);
                gadget_owner.gadgets[0] = Prc::downgrade(&gadget1, j);
                gadget_owner.gadgets[1] = Prc::downgrade(&gadget2, j);
            }

            // Iterate over our `Gadget`s, printing their details out.
            for gadget_weak in gadget_owner.borrow().gadgets.iter() {
                // `gadget_weak` is a `Weak<Gadget>`. Since `Weak` pointers can't
                // guarantee the allocation still exists, we need to call
                // `upgrade`, which returns an `Option<Rc<Gadget>>`.
                //
                // In this case we know the allocation still exists, so we simply
                // `unwrap` the `Option`. In a more complicated program, you might
                // need graceful error handling for a `None` result.

                transaction(|j| {
                    if let Some(gadget) = gadget_weak.upgrade(j) {
                        println!(
                            "Gadget {} owned by {}",
                            gadget.id,
                            gadget.owner.borrow().get_name()
                        );
                    } else {
                        println!("Gadget is already dropped");
                    }
                })
                .unwrap();
            }

            // At the end of the function, `gadget_owner`, `gadget1`, and `gadget2`
            // are destroyed. There are now no strong (`Rc`) pointers to the
            // gadgets, so they are destroyed. This zeroes the reference counter on
            // Gadget Man, so he gets destroyed as well.
        })
        .unwrap();
        print_usage(2);
    }

    pub fn rand() -> u8 {
        use std::fs::File;
        use std::io::Read;
        static mut BUF: [u8; 16] = [0u8; 16];
        static mut IDX: usize = 15;
        unsafe {
            IDX += 1;
            if IDX == 16 {
                IDX = 0;
                let mut f = File::open("/dev/urandom").unwrap();
                f.read_exact(&mut BUF).unwrap();
            }
            BUF[IDX]
        }
    }

    fn may_panic() {
        if rand() > 200_u8 {
            panic!();
        }
    }

    #[test]
    #[allow(clippy::comparison_chain)]
    fn test_dblist() {
        use crate::default::{*, prc::PWeak};

        struct SB {
            root: Prc<PRefCell<Node<i32>>>,
        }
        impl RootObj<A> for SB {
            fn init(j: &Journal) -> Self {
                Self {
                    root: Prc::new(PRefCell::new(Node::new(0)), j),
                }
            }
        }
        struct Node<T: PSafe + std::fmt::Display> {
            value: T,
            next: Option<Prc<PRefCell<Node<T>>>>,
            prev: PWeak<PRefCell<Node<T>>>,
        }
        impl<T: PSafe + std::fmt::Display> Node<T> {
            fn new(x: T) -> Self {
                Self {
                    value: x,
                    next: None,
                    prev: Weak::new(),
                }
            }
        }
        enum Operation {
            AddNewRndNum = 0,
            DelRndNum = 1,
            DelAll = 2,
        }
        use Operation::*;
        let r = rand() % 100;
        let cmd = if r < 20 {
            DelRndNum
        } else if r < 25 {
            DelAll
        } else {
            AddNewRndNum
        };
        let sb = A::open::<SB>("sb11.pool", O_CFNE).unwrap();
        println!();
        print_usage(1);
        transaction(|j| {
            let mut sb = sb.root.borrow_mut(j);
            let value = (rand() % 10) as i32;
            match cmd {
                AddNewRndNum => {
                    println!("adding new element {}", value);
                    let new = Prc::new(PRefCell::new(Node::new(value)),j);
                    if let Some(root) = &sb.next {
                        let mut curr = Prc::downgrade(&root, j);
                        let mut last = PWeak::<PRefCell<Node<i32>>>::new();
                        let mut added = false;
                        while let Some(pnode) = curr.upgrade(j) {
                            let node = pnode.borrow_mut(j);
                            if node.value > value {
                                let mut new_node = (*new).borrow_mut(j);
                                let mut element = node;
                                if rand() % 2 == 0 {
                                    print_usage(2);
                                    println!("############################################################################################# CRASHED 1");
                                    // std::process::exit(0); // 0 to make the test ok
                                }
                                new_node.next = Some(pnode.pclone(j));
                                if let Some(prev) = element.prev.upgrade(j) {
                                    (*prev).borrow_mut(j).next = Some(new.pclone(j));
                                    new_node.prev = Prc::downgrade(&prev, j);
                                } else {
                                    sb.next = Some(new.pclone(j));
                                }
                                element.prev = Prc::downgrade(&new, j);
                                added = true;
                                break;
                            } else if node.value == value {
                                // new should drop
                                added = true;
                                break;
                            }
                            last = Prc::downgrade(&pnode, j);
                            if let Some(next) = node.next.as_ref() {
                                curr = Prc::downgrade(&next, j);
                            } else {
                                curr = Weak::new();
                            }
                        }
                        if !added {
                            if let Some(tail) = last.upgrade(j) {
                                new.borrow_mut(j).prev = Prc::downgrade(&tail, j);
                                tail.borrow_mut(j).next = Some(new);
                            } else {
                                sb.next = Some(new);
                            }
                        }
                    } else {
                        sb.next = Some(new);
                    }
                },
                DelRndNum => {
                    println!("deleting element {}", value);
                    if let Some(root) = &sb.next {
                        let mut curr = Prc::downgrade(&root, j);
                        while let Some(pnode) = curr.upgrade(j) {
                            let node = pnode.borrow();
                            if node.value == value {
                                if let Some(prev) = node.prev.upgrade(j) {
                                    prev.borrow_mut(j).next = node.next.pclone(j);
                                }
                                if let Some(next) = node.next.as_ref() {
                                    next.borrow_mut(j).prev = node.prev.pclone(j);
                                }
                                break;
                            }
                            if let Some(next) = &node.next {
                                curr = Prc::downgrade(&next, j);
                            } else {
                                curr = Weak::new();
                            }
                        }
                    }
                },
                DelAll => {
                    println!("deleting all elements");
                    sb.next = None;
                }
            }
        }).ok();
        println!("strong_count = {}", RootCell::strong_count(&sb));
        print_usage(2);
        let counter = transaction(|j| {
            let mut counter = 0;
            if let Some(root) = &sb.root.borrow().next {
                let mut curr = Prc::downgrade(&root, j);
                print!("[ ");
                while let Some(node) = curr.upgrade(j) {
                    let node = node.borrow();
                    counter += 1;
                    print!("{} ", node.value);
                    if let Some(next) = &node.next {
                        curr = Prc::downgrade(&next, j);
                    } else {
                        break;
                    }
                }
                println!("]");
            }
            counter
        })
        .unwrap();
        println!("total items = {}", counter);
        // assert_eq!(A::used(), 32840 + 64 * counter);
        print_usage(3);
        println!("#############################################################################################");
    }

    // fn formal_verification() {

    //     macro_rules! verify {
    //         ($a:expr) => {
                
    //         };
    //     }

    //     pub trait Invariant {
    //         fn invariant(&self) -> bool;
    //     }

    //     struct Node<T: PartialOrd> {
    //         val: T,
    //         next: Option<Box<Node<T>>>
    //     }

    //     impl<T: PartialOrd> Invariant for Node<T> {
    //         fn invariant(&self) -> bool {
    //             if let Some(n) = &self.next {
    //                 n.val > self.val
    //             } else {
    //                 true
    //             }
    //         }
    //     }

    //     fn main() {
    //         let root = A::open::<Node<i32>>("foo.pool", 0).unwrap();
    //         A::transaction(|j| {
    //             let mut n = root.next.borrow_mut(j);
    //             n.val += 10;
    //             verify!(n.invariant());
    //         }).unwrap();
    //     }
    // }

    #[allow(unused_assignments)]
    #[test]
    fn test_slice() {
        struct MyString<'a> {
            value: &'a [u8],
        }

        impl<'a> MyString<'a> {
            pub fn new(x: &'a str) -> Self {
                Self {
                    value: x.as_bytes(),
                }
            }
        }

        impl<'a> std::ops::AddAssign<&'a str> for MyString<'a> {
            fn add_assign(&mut self, other: &'a str) {
                unsafe {
                    let m: *const [u8] = self.value;
                    let m: *mut [u8] = m as *mut [u8];
                    let m: &mut [u8] = &mut *m;
                    let mut v = m.to_vec();
                    v.append(&mut other.as_bytes().to_vec());
                }
            }
        }
        impl<'a> From<&'a str> for MyString<'a> {
            #[inline]
            fn from(v: &'a str) -> MyString<'a> {
                MyString {
                    value: v.as_bytes(),
                }
            }
        }

        let mut str1 = MyString::new("first");
        str1 += " second";
        println!("str = {:?}", str1.value);

        let mut b1 = Box::new("first");
        *b1 = " second";
        println!("str = {:?}", b1);
    }

    #[test]
    fn test_transaction() {
        use crate::default::*;

        let _heap = A::open_no_root("nosb.pool", O_CF);
        A::transaction(|j| {
            let data = Pbox::new(PRefCell::new(1), j);
            let mut data = data.borrow_mut(j);
            *data = 2;
            println!("data = {}", data);
        })
        .unwrap();
    }

    #[test]
    fn test_logs() {
        use crate::default::*;

        struct Root {
            head: Option<PRefCell<i32>>,
        }
        impl Default for Root {
            fn default() -> Self {
                Root { head: None }
            }
        }
        let root = A::open::<Pbox<PRefCell<Root>>>("sb7.pool", O_CFNE).unwrap();
        let data = A::transaction(|j| {
            let mut root = root.borrow_mut(j);
            if let Some(obj) = &root.head {
                let mut obj = obj.borrow_mut(j);
                *obj += 1;
                *obj
            } else {
                // let _root = root.borrow_mut();
                // let _new_node = PRefCell::new(1);
                // std::process::exit(0);

                let new_node = PRefCell::new(1); //std::process::exit(0)
                root.head = Some(new_node);
                1
            }
        })
        .unwrap();
        println!("data = {}", data);
    }

    // Parc tests

    use crate::boxed::Pbox;
    use crate::sync::Parc;

    use std::thread;

    #[test]
    #[ignore]
    fn inner_tx() {
        use std::clone::Clone as StdClone;
        use std::sync::mpsc::channel;
        const N: usize = 10;

        Heap::transaction(|j| {
            // Spawn a few threads to increment a shared variable (non-atomically), and
            // let the main thread know once all increments are done.
            //
            // Here we're using an Arc to share memory among threads, and the data inside
            // the Arc is protected with a mutex.
            let data = Parc::new(PMutex::new(0), j);
            let (tx, rx) = channel();
            for _ in 0..N {
                let (data, tx) = (data.demote(), tx.clone());
                thread::spawn(move || {
                    // The shared state can only be accessed once the lock is held.
                    // Our non-atomic increment is safe because we're the only thread
                    // which can access the shared state when the lock is held.
                    //
                    // We unwrap() the return value to assert that we are not expecting
                    // threads to ever fail while holding the lock.
                    let res = Heap::transaction(|j| {
                        let data = data.promote(j).unwrap();
                        let mut data = data.lock(j);
                        *data += 1;
                        *data
                    })
                    .unwrap();
                    // the lock is unlocked here when the transaction commits.

                    if res == N {
                        tx.send(()).unwrap();
                    }
                });
            }
            rx.recv().unwrap();
        })
        .unwrap();
    }


    #[test]
    fn parc_heap() {
        Heap::transaction(|j| {
            // the Arc is protected with a mutex.
            let _data = Parc::new(10, j);
            let _weak_five = Parc::downgrade(&_data, j);
        }).unwrap();
    }

    #[test]
    fn test_parallel_alloc() {
        let mut threads = vec![];
        struct Node {
            id: i32,
            next: Option<Parc<Node, A>>,
        }
        struct Root {
            root: Parc<PMutex<Option<Node>, A>, A>,
        }
        impl RootObj<A> for Root {
            fn init(j: &Journal<A>) -> Self {
                let node = Node { id: 0, next: None };
                Self {
                    root: Parc::new(PMutex::new(Some(node)), j),
                }
            }
        }
        let root = A::open::<Root>("sb8.pool", O_CFNE).unwrap();
        print_usage(0);
        // let prev = A::used();
        for i in 0..5 {
            let root = root.root.demote();
            threads.push(thread::spawn(move || {
                A::transaction(|j| {
                    if let Some(root) = root.promote(j) {
                        let node = Node { id: i, next: None };
                        let mut root = root.lock(j);
                        *root = Some(node);
                    }
                })
                .unwrap()
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        print_usage(1);

        // assert_eq!(A::used(), prev);
    }

    #[test]
    fn test_mutex() {
        use crate::default::*;

        struct SB {
            root: Option<Prc<PMutex<i32>>>,
        }
        impl Default for SB {
            fn default() -> Self {
                SB { root: None }
            }
        }
        let sb = A::open::<Pbox<PRefCell<SB>>>("sb9.pool", O_CFNE).unwrap();
        transaction(|j| {
            let mut sb = sb.borrow_mut(j);
            if sb.root.is_none() {
                sb.root = Some(Prc::new(PMutex::new(12), j));
            }
            let mut val = sb.root.as_ref().unwrap().lock(j);
            println!("data is {}", val);
            *val = (rand() % 100) as i32;
            println!("data is {}", val);
            *val = (rand() % 100) as i32;
            println!("data is {}", val);
        })
        .unwrap();
    }

    #[test]
    fn test_mutex_mt() {
        struct SB {
            value: Parc<PMutex<i32, A>, A>,
        }
        impl RootObj<A> for SB {
            fn init(j: &stm::Journal<A>) -> Self {
                SB {
                    value: Parc::new(PMutex::new(0), j),
                }
            }
        }
        // for _ in 0..100 {
        let sb = A::open::<SB>("sb10.pool", O_CFNE).unwrap();
        let mut threads = vec![];
        print_usage(0);
        let mut v = vec![];
        for _ in 0..10 {
            v.push((rand() % 100) as i32);
        }
        let mut sum = A::transaction(|j| *sb.value.lock(j)).unwrap();
        println!("sum = {}", sum);
        // let mutex = std::sync::Arc::new(std::sync::Mutex::new(()));
        for i in 0..5 {
            let v = v.clone();
            sum += v[i] + v[5 + i];
            let sb = sb.clone();
            let value = sb.value.demote();
            threads.push(std::thread::spawn(move || {
                // let _lock = mutex.lock();
                let delta1 = v[i];
                let delta2 = v[5 + i];
                let res = A::transaction(|j| {
                    if let Some(val) = value.promote(j) {
                        {
                            // use std::io::Write;
                            // std::io::stdout().flush().unwrap();
                            // std::process::exit(0);
                            let mut val = val.lock(j);
                            println!("{} + {} = {}", *val, delta1, *val + delta1);
                            *val += delta1 - 10;
                            *val += 10;
                        }

                        // no other thread can access sb.value here even the
                        // fact that the lock guard is out of scope.

                        {
                            let mut val = val.lock(j);
                            println!("{} + {} = {}", *val, delta2, *val + delta2);
                            *val += delta2 - 10;
                            // if i == 2 {
                            //     panic!("intentional {}", delta2);
                            //     // std::process::exit(0);
                            // }
                            *val += 10;
                        }
                    }
                });
                if res.is_err() {
                    // both changes are discarded
                    delta1 + delta2
                } else {
                    0
                }
            }));
        }
        let mut missed: i32 = 0;
        for t in threads {
            match t.join() {
                Ok(delta) => missed += delta,
                Err(e) => println!("{:?}", e),
            }
        }
        println!("{:?}", v);
        let fin = A::transaction(|j| *sb.value.lock(j)).unwrap();

        println!(
            "sum={}, fin={}, missed={}, fin+missed={}",
            sum,
            fin,
            missed,
            fin + missed
        );
        assert_eq!(sum, fin + missed);
        print_usage(1);
        // }
    }

    #[test]
    fn test_tramutex_mt() {
        struct SB {
            value: Parc<PMutex<i32, A>, A>,
        }
        impl RootObj<A> for SB {
            fn init(j: &stm::Journal<A>) -> Self {
                SB {
                    value: Parc::new(PMutex::new(0), j),
                }
            }
        }
        // for _ in 0..100 {
        let sb = A::open::<SB>("sb20.pool", O_CFNE).unwrap();
        
        let mut threads = vec![];
        print_usage(0);
        let mut v = vec![];
        for _ in 0..10 {
            v.push((rand() % 100) as i32);
        }
        let mut sum = A::transaction(|j| *sb.value.lock(j)).unwrap();
        println!("sum = {}", sum);
        // let mutex = std::sync::Arc::new(std::sync::Mutex::new(()));
        for i in 0..3 {
            let v = v.clone();
            sum += v[i] + v[5 + i];
            let sb = sb.clone();
            let value = sb.value.demote();
            threads.push(std::thread::spawn(move || {
                // let _lock = mutex.lock();
                let delta1 = v[i];
                let delta2 = v[5 + i];
                let res = A::transaction(|j| {
                    if let Some(val) = value.promote(j) {
                        {
                            // use std::io::Write;
                            // std::io::stdout().flush().unwrap();
                            // std::process::exit(0);
                            let mut val = val.lock(j);
                            println!("{} + {} = {}", *val, delta1, *val + delta1);
                            *val += delta1 - 10;
                            *val += 10;
                        }

                        {
                            let mut val = val.lock(j);
                            println!("{} + {} = {}", *val, delta2, *val + delta2);
                            *val += delta2 - 10;
                            // if i == 2 {
                            //     panic!("intentional {}", delta2);
                            //     // std::process::exit(0);
                            // }
                            *val += 10;
                        }

                        if i == 2 {
                            panic!("intentional {}", delta2);
                            // std::process::exit(0);
                        }
                    }
                });
                if res.is_err() {
                    // both changes are discarded
                    delta1 + delta2
                } else {
                    0
                }
            }));
        }
        let mut missed: i32 = 0;
        for t in threads {
            match t.join() {
                Ok(delta) => missed += delta,
                Err(e) => println!("{:?}", e),
            }
        }
        println!("{:?}", v);
        let fin = A::transaction(|j| *sb.value.lock(j)).unwrap();

        println!(
            "sum={}, fin={}, missed={}, fin+missed={}",
            sum,
            fin,
            missed,
            fin + missed
        );
        assert_eq!(sum, fin + missed);
        print_usage(1);
        // }
    }

    // #[test]
    // fn inter_pool() {
    //     crate::pool!(pool1);
    //     crate::pool!(pool2);

    //     type P1 = pool1::BuddyAlloc;
    //     type P2 = pool2::BuddyAlloc;

    //     type Root = Pbox<PRefCell<Option<Pbox<i32,P2>>,P1>,P1>;
    //     let root = P1::open::<Root>("interpool.pool", O_CFNE).unwrap();
    //     let _p = P2::open_no_root("interpool2.pool", O_CFNE).unwrap();
    //     let _ = P2::transaction(|j2| {
    //         let mut root = root.borrow_mut(j2);
    //         let b: Pbox<PRefCell<Option<Pbox<i32,P2>>,P1>,P1>;
    //         b = Pbox::new(PRefCell::new(None, j1), j1);
    //     });
    // }

    #[test]
    fn propagate_panic() {
        use crate::default::*;

        let _image = A::open_no_root("nosb.pool", O_CF);
        if A::transaction(|j| {
            let ptr = Parc::new(PMutex::new(1), j);
            print_usage(1);
            let _ = transaction(|j| {
                let _k = ptr.pclone(j);
                println!("panicking!");
                panic!("yes");
            });
            println!("out 1!");
        })
        .is_err()
        {
            println!("panicking");
        }

        // assert_eq!(A::used(), A::minimal_usage());

        // let p = ptr.clone();
        // transaction(|_| {
        //     let _k = p.clone();
        //     // panic!("yes");
        // });
    }

    #[test]
    #[should_panic]
    fn outside_tx() {
        use crate::default::*;

        let _image = A::open_no_root("nosb.pool", O_CF);
        let b = PRefCell::new(10);
        let c = PCell::new(10);
        let m = PMutex::new(10);
        A::transaction(|j| {
            let mut b = b.borrow_mut(j);
            *b = 20;
            c.set(20, j);
            let mut m = m.lock(j);
            *m = 20;
        }).unwrap();
    }
}

#[cfg(test)]
mod test_btree {

    //! btree.rs -- textbook implementation of btree /w preemptive splitting
    //! equivalent to [btree example] from PMDK.
    //!
    //! [btree example]: https://github.com/pmem/pmdk/blob/master/src/examples/libpmemobj/tree_map/btree_map.c

    use crate::default::*;
    use std::cell::RefCell;
    use std::fmt::{Display, Error, Formatter};
    use std::rc::*;
    // use crate::cell::*;
    // use crate::Map;

    type P = BuddyAlloc;

    const N: usize = 5;

    #[derive(Clone)]
    pub struct NodeItem<K, V> {
        key: K,
        val: V,
    }

    impl<K: Display, V: Display> Display for NodeItem<K, V> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            write!(f, "{} ", self.key)
        }
    }

    impl<K: Default, V: Default> Default for NodeItem<K, V> {
        fn default() -> Self {
            Self {
                key: K::default(),
                val: V::default(),
            }
        }
    }

    #[derive(Clone)]
    pub struct LeafNode<K, V> {
        len: usize,
        left: Weak<RefCell<BTreeNode<K, V>>>,
        right: Weak<RefCell<BTreeNode<K, V>>>,
        values: [Option<NodeItem<K, V>>; N - 1],
    }

    impl<K: Copy, V> LeafNode<K, V> {
        fn key(&self) -> K {
            self.values[0].as_ref().unwrap().key
        }
    }

    impl<K: Default + PartialOrd + Copy, V: Default + Copy> LeafNode<K, V> {
        fn add(&mut self, item: NodeItem<K, V>) {
            for i in 0..self.len {
                if let Some(value) = &self.values[i] {
                    if value.key > item.key {
                        self.values[i..].rotate_right(1);
                        self.values[i] = Some(item.clone());
                        self.len += 1;
                        return;
                    }
                }
            }
            self.values[self.len] = Some(item.clone());
            self.len += 1;
        }
    }

    #[derive(Clone)]
    pub struct IntNode<K, V> {
        len: usize,
        left: Weak<RefCell<BTreeNode<K, V>>>,
        right: Weak<RefCell<BTreeNode<K, V>>>,
        keys: [K; N - 1],
        slots: [Option<Rc<RefCell<BTreeNode<K, V>>>>; N],
    }

    impl<K: Copy, V> IntNode<K, V> {
        fn key(&self) -> K {
            self.keys[0]
        }
    }

    impl<K: Default + PartialOrd + Copy, V: Default + Copy> IntNode<K, V> {
        fn add(&mut self, node: Rc<RefCell<BTreeNode<K, V>>>) {
            let key = node.borrow().key();
            for i in 0..self.len {
                if self.keys[i] > key {
                    self.keys[i..].rotate_right(1);
                    self.slots[i..].rotate_right(1);
                    self.keys[i] = key;
                    self.slots[i] = Some(node);
                    self.len += 1;
                    return;
                }
            }
            self.slots[self.len] = Some(node);
            self.len += 1;
        }
    }

    #[derive(Clone)]
    pub enum BTreeNode<K, V> {
        Leaf(LeafNode<K, V>),
        Internal(IntNode<K, V>),
    }

    use BTreeNode::*;

    impl<K: Default, V: Default> BTreeNode<K, V> {
        fn leaf() -> Self {
            Leaf(LeafNode {
                len: 0,
                left: Weak::new(),
                right: Weak::new(),
                values: Default::default(),
            })
        }

        fn internal() -> Self {
            Internal(IntNode {
                len: 0,
                left: Weak::new(),
                right: Weak::new(),
                keys: Default::default(),
                slots: Default::default(),
            })
        }
    }

    impl<K: Default + PartialOrd + Copy, V: Default + Copy> BTreeNode<K, V> {
        fn len(&self) -> usize {
            match self {
                Leaf(this) => this.len,
                Internal(this) => this.len,
            }
        }

        fn insert(
            p: &Rc<RefCell<BTreeNode<K, V>>>,
            item: NodeItem<K, V>,
        ) -> Option<BTreeNode<K, V>> {
            // TODO: do not doublicate key (for publication)
            let mut this = p.borrow_mut();
            match &mut *this {
                Leaf(this) => {
                    for i in 0..this.len {
                        if let Some(value) = &mut this.values[i] {
                            if value.key == item.key {
                                value.val = item.val;
                                return None;
                            }
                        }
                    }
                    if this.len < N - 2 {
                        this.add(item);
                        None
                    } else {
                        if let Some(left) = this.left.upgrade() {
                            let sib = left.borrow();
                            if sib.len() < N - 1 {
                                let smallest = this.values[0].as_ref().unwrap().clone();
                                if item.key <= smallest.key {
                                    Self::insert(&left, item);
                                } else {
                                    Self::insert(&left, smallest);
                                    this.values.rotate_left(1);
                                    this.values[N - 2] = None;
                                    this.len -= 1;
                                    this.add(item);
                                }
                                return None;
                            }
                        }
                        if let Some(right) = this.right.upgrade() {
                            let sib = right.borrow();
                            if sib.len() < N - 1 {
                                let largest = this.values[N - 2].as_ref().unwrap().clone();
                                if item.key >= largest.key {
                                    Self::insert(&right, item);
                                } else {
                                    Self::insert(&right, largest);
                                    this.values[N - 2] = None;
                                    this.len -= 1;
                                    this.add(item);
                                }
                                return None;
                            }
                        }

                        // should split
                        let mut new = LeafNode::<K, V> {
                            len: 0,
                            left: Weak::new(),
                            right: Weak::new(),
                            values: Default::default(),
                        };
                        let mid = N / 2;
                        let cmp = this.values[mid - 1].as_ref().unwrap().key;
                        for i in 0..mid {
                            new.values[i] = this.values[mid + i].clone();
                            this.values[mid + i] = None;
                        }
                        this.len = mid;
                        new.len = mid;
                        if item.key <= cmp {
                            this.add(item);
                        } else {
                            new.add(item);
                        }
                        let mut parent = IntNode::<K, V> {
                            len: 1,
                            left: Weak::new(),
                            right: Weak::new(),
                            keys: Default::default(),
                            slots: Default::default(),
                        };
                        let new_key = new.key();
                        let this_key = this.key();
                        if this_key <= new_key {
                            new.left = Rc::downgrade(p);
                            new.right = this.right.clone();
                        } else {
                            new.left = this.left.clone();
                            new.right = Rc::downgrade(p);
                        }
                        let new = Rc::new(RefCell::new(Leaf(new)));
                        if this_key <= new_key {
                            this.right = Rc::downgrade(&new);
                        } else {
                            this.left = Rc::downgrade(&new);
                        }
                        let slf = Rc::new(RefCell::new(Leaf(this.clone())));
                        parent.keys[0] = if this_key <= new_key {
                            parent.slots[0] = Some(slf);
                            parent.slots[1] = Some(new);
                            new_key
                        } else {
                            parent.slots[0] = Some(new);
                            parent.slots[1] = Some(slf);
                            this_key
                        };
                        Some(Internal(parent))
                    }
                }
                Internal(_) => None,
            }
        }
    }

    pub struct BTree<K, V> {
        root: Rc<RefCell<BTreeNode<K, V>>>,
    }

    impl<K: Default, V: Default> Default for BTree<K, V> {
        fn default() -> Self {
            Self {
                root: Rc::new(RefCell::new(BTreeNode::leaf())),
            }
        }
    }

    impl<K: Copy, V> BTreeNode<K, V> {
        fn key(&self) -> K {
            match self {
                Leaf(this) => this.key(),
                Internal(this) => this.key(),
            }
        }
    }

    impl<K: Display, V: Display> Display for BTreeNode<K, V> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            match self {
                Leaf(this) => {
                    for i in 0..N - 1 as usize {
                        if let Some(value) = &this.values[i] {
                            value.fmt(f)?;
                        } else {
                            break;
                        }
                    }
                }
                Internal(this) => {
                    for i in 0..N - 1 as usize {
                        if let Some(child) = &this.slots[i] {
                            child.borrow().fmt(f)?;
                        } else {
                            break;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    impl<K: PartialOrd, V> BTree<K, V> {
        pub fn is_empty(&self) -> bool {
            if let Leaf(root) = &*self.root.borrow() {
                root.values[0].is_none()
            } else {
                false
            }
        }
    }

    impl<K: PartialOrd + Default + Copy, V: Default + Copy> BTree<K, V> {
        pub fn insert(&mut self, _key: K, _val: V) {
            // BTreeNode::insert(&self.root, key, val);
        }
    }

    impl<K: Display, V: Display> Display for BTree<K, V> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            self.root.borrow().fmt(f)
        }
    }

    #[test]
    fn test_btree() {
        let mut btree = BTree::<u64, u64>::default();
        btree.insert(10, 0);
        btree.insert(30, 0);
        btree.insert(20, 0);
        btree.insert(25, 0);
        btree.insert(50, 0);

        println!("{}", btree);
    }
}

mod temp_test {
    #[test]
    fn abort_prc() {
        use std::mem::drop;
        use crate::default::*;
        type P = BuddyAlloc;
        let obj = P::open::<Root>("foo.pool", O_CF).unwrap();

        struct Root(PRefCell<Option<Parc<i32>>>);
        impl RootObj<P> for Root {
            fn init(j: &Journal) -> Self {
                Root(PRefCell::new(Some(Parc::new(10, j))))
            }
        }

        let vweak_obj = obj.0.borrow().as_ref().unwrap().demote();
        
        P::transaction(|j| {
            let strong_obj = vweak_obj.promote(j);
            assert!(strong_obj.is_some());
            
            // Destroy all strong pointers.
            drop(strong_obj);
            *obj.0.borrow_mut(j) = None;
        
            assert!(vweak_obj.promote(j).is_none());
        }).unwrap();
    }
}