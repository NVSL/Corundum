fn main() {
    use corundum::default::*;
    use corundum::stat::*;
    use std::env;

    type P = BuddyAlloc;

    struct Root {
        list: PVec<PCell<i32>>
    }

    impl RootObj<P> for Root {
        fn init(j: &Journal) -> Self {
            let mut list = PVec::with_capacity(3000, j);
            for i in 0..3000 {
                list.push(PCell::new(i), j);
            }
            Self { list }
        }
    }
    use std::vec::Vec as StdVec;

    let args: StdVec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} file-name", args[0]);
        return;
    }

    let root = P::open::<Root>(&args[1], O_CF).unwrap();

    for c in &[10, 100, 500, 1000, 2000, 3000] {
        let s = format!("Transaction Size {:4}", c);
        measure!(s, {
            transaction(|j| {
                for i in 0..*c {
                    root.list[i].set(root.list[(i + 1) % *c].get(), j);
                }
            }).unwrap();
        });
    }

    println!("{}", report());
}