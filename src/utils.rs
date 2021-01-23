use std::fmt::{Debug, Error, Formatter};
use std::fs::File;
use std::io::Read;

#[cfg(not(feature = "no_flush_alloc"))]
use crate::ll::*;

pub fn rand() -> i64 {
    let mut buf: [u8; 8] = [0u8; 8];
    let mut f = File::open("/dev/urandom").unwrap();
    f.read_exact(&mut buf).unwrap();
    i64::from_be_bytes(buf)
}

static mut CRASH_AT: Option<u32> = None;
static mut CRASH_ALLOWED: bool = false;

pub fn allow_crash(v: bool) {
    unsafe {
        CRASH_ALLOWED = v;
    }
}

pub fn may_crash(ln: u32, cnt: i32) {
    unsafe {
        if !CRASH_ALLOWED {
            return;
        }
        if let Some(line) = CRASH_AT {
            if ln == line {
                static mut COUNT: i32 = 0;
                COUNT += 1;
                // if COUNT == cnt {
                // if rand() % 3 == 0 {
                println!("Crashed at line {}", ln);
                std::process::exit(0);
                // }
            }
        } else {
            for (key, value) in std::env::vars() {
                if key == "CRASH_AT" {
                    let line: u32 = value.parse().unwrap_or(u32::MAX);
                    CRASH_AT = Some(line);
                    may_crash(ln, cnt);
                    return;
                }
            }
            CRASH_AT = Some(u32::MAX);
        }
    }
}

pub fn as_slice<T: ?Sized>(x: &T) -> &[u8] {
    let ptr: *const T = x;
    let ptr: *const u8 = ptr as *const u8;  // cast from ptr-to-SomeStruct to ptr-to-u8
    unsafe {
        std::slice::from_raw_parts(ptr, std::mem::size_of_val(x))
    }
}

pub fn as_slice64<T: ?Sized>(x: &T) -> &[u64] {
    let ptr: *const T = x;
    let ptr: *const u64 = ptr as *const u64;  // cast from ptr-to-SomeStruct to ptr-to-u8
    unsafe {
        std::slice::from_raw_parts(ptr, std::mem::size_of_val(x)/8)
    }
}

#[repr(C)]
pub struct Ring<T, const N: usize> {
    data: [T; N],
    head: usize,
    tail: usize,
}

impl<T, const N: usize> Ring<T, N> {
    pub fn new() -> Self {
        unsafe {
            Self {
                data: std::mem::zeroed(),
                head: 0,
                tail: 0,
            }
        }
    }

    #[inline]
    pub fn push(&mut self, x: T) {
        debug_assert!(
            (self.tail+1)%N != self.head,
            format!("too many slots are used (len = {})", N)
        );

        self.data[self.tail] = x;
        self.tail = (self.tail + 1) % N;
    }

    #[inline]
    pub fn push_sync(&mut self, x: T) {
        debug_assert!(
            (self.tail+1)%N != self.head,
            format!("too many slots are used (len = {})", N)
        );
        self.data[self.tail] = x;

        #[cfg(not(feature = "no_flush_alloc"))]
        persist(&self.data[self.tail], 8);
        
        self.tail = (self.tail + 1) % N;

        #[cfg(not(feature = "no_flush_alloc"))]
        persist(&self.head, 16);
    }

    #[inline]
    pub fn sync_all(&self) {
        if self.head == self.tail {
            #[cfg(not(feature = "no_flush_alloc"))]
            persist(&self.head, 16);
            return;
        }
        #[cfg(not(feature = "no_flush_alloc"))]
        {
            let h = &self.data[self.head] as *const _ as usize;
            let t = &self.data[self.tail] as *const _ as usize;
            if h < t {
                persist(&self.data[self.head], t - h);
                persist(&self.head, 16);
            } else {
                let b = self as *const Self as usize;
                persist(self, h - b);
                let b = b + std::mem::size_of::<Self>();
                persist(&self.data[self.tail], b - t);
            }
        }
    }

    #[inline]
    pub fn contains(&self, x: T)-> bool where T: Eq {
        let mut head = self.head;
        while head != self.tail {
            if x == self.data[head] {
                return true;
            }
            head = (head + 1) % N;
        }
        false
    }

    #[inline]
    pub fn clear(&mut self) {
        self.head = self.tail;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    #[inline]
    pub fn len(&self) -> usize {
        ((self.tail + N) - self.head) % N
    }
}

impl<T: Copy, const N: usize> Ring<T, N> {
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.head == self.tail {
            None
        } else {
            let res = Some(self.data[self.head]);
            self.head = (self.head + 1) % N;
            res
        }
    }

    #[inline]
    pub fn foreach<F: Fn(T) -> ()>(&mut self, f: F) {
        let mut head = self.head;
        while head != self.tail {
            f(self.data[head]);
            head = (head + 1) % N;
        }
    }

    #[inline]
    pub fn drain_atomic<F: Fn(T), E: Fn()>(&mut self, f: F, end: E) {
        while self.head != self.tail {
            f(self.data[self.head]);
            self.head = (self.head + 1) % N;
            end();
        }
    }

    #[inline]
    pub fn foreach_reverse<F: Fn(T) -> ()>(&mut self, f: F) {
        let mut tail = self.tail;
        while tail != self.head {
            let d = self.data[tail];
            f(d);
            tail = (tail + N - 1) % N;
        }
    }

    #[inline]
    pub fn find<F: Fn(T) -> bool>(&self, f: F) -> bool {
        let mut head = self.head;
        while head != self.tail {
            let d = self.data[head];
            if f(d) {
                return true;
            }
            head = (head + 1) % N;
        }
        false
    }
}

impl<T: Debug, const N: usize> Debug for Ring<T, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{{len: {}, [", self.len())?;
        let mut head = self.head;
        while head != self.tail {
            write!(f, "{:?}", self.data[head])?;
            head = (head + 1) % N;
            if head != self.tail {
                write!(f, ", ")?;
            }
        }
        write!(f, "]}}")
    }
}

mod test {
    #![allow(unused)]
    use super::Ring;

    #[test]
    fn ring_buffer() {
        let mut rng: Ring<i32, 8> = Ring::new();

        for i in 1..8 {
            rng.push(i);
            println!("{:?}", rng);
        }

        rng.foreach(|x| {
            println!("{}", x);
        });
    }
}

#[macro_export]
macro_rules! log {
    ($p:tt, $c:tt, $tag:expr, $msg:expr, $($args:tt)*) => {
        #[cfg(feature = "verbose")] {
            use term_painter::Color::*;
            use term_painter::ToStyle;

            println!("{:<8} {}", $p::name().to_owned() + ":",
                $c.paint(format!("{:>10}  {}", $tag, format!($msg, $($args)*))));
        }
    };
}