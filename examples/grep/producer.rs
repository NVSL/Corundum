use crate::stack::Stack;
use corundum::default::*;
use corundum::stm::Journal;
use corundum::vec::Vec;
use corundum::sync::VWeak;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::string::String as StdStr;
use std::vec::Vec as StdVec;

type P = BuddyAlloc;
const BATCH_SIZE: usize = 1024; // number of chars per job

pub struct Producer {
    filenames: PVec<PString>,
    // 0: file index, 1: line number
    pos: PMutex<(usize, u64)>,
    lines: Parc<PMutex<Stack<PString>>>,
}

impl Producer {
    pub fn new(
        filelist: StdVec<StdStr>,
        lines: Parc<PMutex<Stack<PString>>>,
        j: &Journal<P>,
    ) -> Self {
        let mut filenames = Vec::with_capacity(filelist.len(), j);
        for filename in filelist {
            filenames.push(filename.to_pstring(j), j);
        }
        Self {
            filenames,
            lines,
            pos: PMutex::new((0, 0), j),
        }
    }

    /// Starts reading the files and adding to the `lines`
    pub fn start(this: VWeak<Self, P>) {
        loop {
            if !P::transaction(|j| {
                if let Some(this) = this.upgrade(j) {
                    let mut pos = this.pos.lock(j);
                    if pos.0 < this.filenames.len() {
                        let filename = &this.filenames[pos.0];
                        let mut f =
                            BufReader::new(File::open(filename.as_str()).expect("open failed"));
                        let mut read = 0;
                        if f.seek(SeekFrom::Start(pos.1)).is_ok() {
                            let mut buf = StdVec::<u8>::new();
                            let mut line = StdVec::<u8>::new();
                            let mut lines = this.lines.lock(j);
                            loop {
                                let r = f.read_until(b'\n', &mut buf).expect("read_until failed");
                                if r != 0 {
                                    read += r;
                                    line.append(&mut buf);
                                    if read >= BATCH_SIZE {
                                        let s = StdStr::from_utf8(line).expect("from_utf8 failed");
                                        lines.push(PString::from_str(&s, j), j);
                                        pos.1 += read as u64;
                                        break;
                                    }
                                } else {
                                    if !line.is_empty() {
                                        let s = StdStr::from_utf8(line).expect("from_utf8 failed");
                                        lines.push(PString::from_str(&s, j), j);
                                    }
                                    pos.1 = 0;
                                    pos.0 += 1;
                                    break;
                                }
                            }
                        } else {
                            pos.1 = 0;
                            pos.0 += 1;
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .unwrap()
            {
                return;
            }
        }
    }
}
