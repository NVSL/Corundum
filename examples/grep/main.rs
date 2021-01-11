#![allow(dead_code)]
//! # Word Count example with MapReduce model

mod consumer;
mod hashmap;
mod producer;
mod stack;

use consumer::Consumer;
use hashmap::*;
use corundum::default::*;
use producer::Producer;
use stack::*;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::thread;

type P = BuddyAlloc;

fn help() {
    println!("usage: grep [OPTIONS] list-file");
    println!();
    println!("OPTIONS:");
    println!("  -p num        Search pattern (Default '\\w+')");
    println!("  -r num        Number of reader threads (Default 1)");
    println!("  -c num        Number of consumer threads (Default 1)");
    println!("  -f file       Pool filename (Default ./wc.pool)");
    println!("  -C            Continue from the previous run");
    println!("  -P            Prepare only (do not run threads)");
    println!("  -h            Display help");
    println!();
    println!("The input list-file should contain a list files to read and count words.");
}

fn main() {
    let args: Vec<std::string::String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} [OPTIONS] filename", args[0]);
        return;
    }

    let mut r = 1;
    let mut c = 1;
    let mut pool = "wc.pool".to_string();
    let mut filename = std::string::String::new();
    let mut i = 1;
    let mut cont = false;
    let mut prep = false;
    let mut pattern = "(\\w+)".to_string();
    while i < args.len() {
        let s = &args[i];
        if s == "-h" {
            help();
            return;
        } else if s == "-p" {
            if i == args.len() - 1 {
                panic!("-p requires an argument");
            }
            i += 1;
            pattern = format!("({})", args[i]);
        } else if s == "-r" {
            if i == args.len() - 1 {
                panic!("-r requires an argument");
            }
            i += 1;
            r = args[i].parse().expect("An integer expected");
            if r < 1 {
                panic!("Number of reader threads cannot be less than 1");
            }
        } else if s == "-c" {
            if i == args.len() - 1 {
                panic!("-c requires an argument");
            }
            i += 1;
            c = args[i].parse().expect("An integer expected");
            if c < 1 {
                panic!("Number of consumer threads cannot be less than 1");
            }
        } else if s == "-f" {
            if i == args.len() - 1 {
                panic!("-f requires an argument");
            }
            i += 1;
            pool = args[i].clone();
        } else if s == "-C" {
            cont = true;
        } else if s == "-P" {
            prep = true;
        } else if filename.is_empty() {
            filename = s.clone();
        } else {
            panic!(format!("Unknown option `{}'", s));
        }
        i += 1;
    }

    struct Root {
        lines: Parc<PMutex<Stack<PString>>>,
        words: Parc<PMutex<HashMap<PString, u64>>>,
        producers: PRefCell<PVec<Parc<Producer>>>,
        consumers: PRefCell<PVec<Parc<Consumer>>>,
    }

    impl RootObj<P> for Root {
        fn init(j: &Journal) -> Self {
            Root {
                lines: Parc::new(PMutex::new(Stack::new(), j), j),
                words: Parc::new(PMutex::new(HashMap::new(j), j), j),
                producers: PRefCell::new(PVec::new(j), j),
                consumers: PRefCell::new(PVec::new(j), j),
            }
        }
    }
    
    let root = P::open::<Root>(&pool, O_CFNE | O_8GB).unwrap();

    P::transaction(|j| {
        let mut producers = root.producers.borrow_mut(j);
        let mut consumers = root.consumers.borrow_mut(j);

        if !cont {
            producers.clear();
            consumers.clear();

            root.lines.lock(j).clear();
            root.words.lock(j).clear(j);

            let mut files = vec![];
            let f = BufReader::new(
                File::open(&filename).expect(&format!("cannot open `{}`", &filename)),
            );

            for line in f.lines() {
                files.push(line.unwrap());
            }
            let p = usize::min(r, files.len());
            let b = files.len() / p;
            for i in 0..p + 1 {
                if i * b < files.len() {
                    producers.push(
                        Parc::new(
                            Producer::new(
                                files[i * b..usize::min(files.len(), (i + 1) * b)].to_vec(),
                                root.lines.pclone(j),
                                j,
                            ),
                            j,
                        ),
                        j,
                    );
                }
            }
            for _ in 0..c {
                consumers.push(
                    Parc::new(
                        Consumer::new(&pattern, root.lines.pclone(j), j),
                        j,
                    ),
                    j,
                );
            }
        }
    }).unwrap();

    eprintln!(
        "Total remaining from previous run: {} ",
        P::transaction(|j| root.lines.lock(j).len()).unwrap()
    );

    if !prep {
        let producers = root.producers.borrow();
        let consumers = root.consumers.borrow();

        let mut p_threads = vec![];
        let mut c_threads = vec![];

        for p in &*producers {
            let p = p.demote();
            p_threads.push(thread::spawn(move || Producer::start(p)))
        }

        for c in &*consumers {
            let c = c.demote();
            c_threads.push(thread::spawn(move || Consumer::start(c)))
        }

        for thread in p_threads {
            thread.join().unwrap()
        }

        // Notifying consumers that there is no more feeds
        let consumers = root.consumers.borrow();
        for consumer in &*consumers {
            consumer.stop_when_finished();
        }

        for thread in c_threads {
            thread.join().unwrap()
        }

        // Display results
        P::transaction(|j| {
            for c in &*consumers {
                c.collect(root.words.pclone(j), j);
            }
            let words = root.words.lock(j);
            println!("{}", words);
        }).unwrap();
    }
    println!("Memory usage = {} bytes", P::used());
}
