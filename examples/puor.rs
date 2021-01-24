use corundum::default::*;

type P = BuddyAlloc;

struct Consumer {
    buf: PMutex<u64>,
    volume: Parc<PMutex<u64>>
}

#[derive(Root)]
struct Root {
    volume: Parc<PMutex<u64>>,
    consumers: PRefCell<PVec<Parc<Consumer>>>
}

fn main() {
    use std::env;
    use std::vec::Vec as StdVec;

    let mut args: StdVec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} file-name [OPTIONS]", args[0]);
        println!("OPTIONS:");
        println!("  -i <num>   Initial value (default: 1000000)");
        println!("  -t <num>   Number of threads (default: 4)");
        println!("  -r         Reset");
        return;  
    }

    let root = P::open::<Root>(&args[1], O_CFNE | O_4GB).unwrap();
    let mut resetting = false;
    let mut init_val = 1000000;
    let mut t = root.consumers.borrow().len();

    args.remove(0);
    args.remove(0);
    while !args.is_empty() {
        if args[0] == "-i" && args.len() > 1 {
            args.remove(0);
            init_val = args[0].parse::<u64>().expect("Error: Expected a number");
        } else if args[0] == "-t" && args.len() > 1 {
            args.remove(0);
            t = args[0].parse::<usize>().expect("Error: Expected a number");
            assert!(t>0);
        } else if args[0] == "-r" {
            resetting = true;
        } else {
            panic!("Bad option `{}`", args[0]);
        }
        args.remove(0);
    }

    if resetting || root.consumers.borrow().is_empty() {
        println!("Initializing the consumers to pour {} gallons of oil into {} barrels", init_val, t);
        let r = root.clone();
        P::transaction(move |j| {
            let mut volume = r.volume.lock(j);
            let mut consumers = r.consumers.borrow_mut(j);
            *volume = init_val;
            if t == 0 { t = 4 };
            let quota = init_val / t as u64;
            consumers.clear();
            for _ in 0..t-1 {
                consumers.push(Parc::new(Consumer{
                    buf: PMutex::new(quota),
                    volume: r.volume.pclone(j)
                }, j), j);
                init_val -= quota;
            }
            consumers.push(Parc::new(Consumer{
                buf: PMutex::new(init_val),
                volume: r.volume.pclone(j)
            }, j), j);
        }).unwrap();
    }

    let mut threads = vec![];
    for c in &*root.consumers.borrow() {
        let v = c.demote();
        threads.push(std::thread::spawn(move || {
            loop {
                let v = v.clone();
                let mut b = P::transaction(|j| {
                    if let Some(c) = v.promote(j) {
                        let mut b = c.buf.lock(j);
                        let mut vol = c.volume.lock(j);
                        if *b > 100 {
                            *b -= 100;
                            *vol -= 100;
                            100
                        } else {
                            let r = *b;
                            *vol -= r;
                            *b = 0;
                            r
                        }
                    } else {
                        0
                    }
                }).unwrap();
    
                if b == 0 { break }

                P::transaction(move |j| {
                    if let Some(c) = v.promote(j) {
                        while b > 0 {
                            let vol = c.volume.lock(j);
                            b -= 1;
                            eprint!("\r\x1b[?25lRemaining: {:<12} \x1b[?25h", *vol + b);
                        }
                    }
                }).unwrap();
            }
        }));
    }

    for t in threads {
        t.join().unwrap();
    }
}