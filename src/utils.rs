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

static mut CRASH_PROB: Option<u64> = None;

#[macro_export]
macro_rules! may_crash {
    () => {
        if $crate::utils::can_crash() {
            eprintln!("\nCrashed at {}:{}", file!(), line!());
            std::process::exit(0);
        }
    };
}

#[inline]
pub fn can_crash() -> bool {
    unsafe {
        if let Some(p) = CRASH_PROB {
            if p == 0 {
                return false;
            } else {
                let r: u64 = rand::random();
                return r % 10000 == 0;
            }
        } else {
            let p = std::env::var("CRASH_PROB")
                .unwrap_or("0".to_string())
                .parse::<u64>()
                .expect("CRASH_PROB should be a non-negative integer");
            CRASH_PROB = Some(p);
        }
        can_crash()
    }
}

#[inline]
#[doc(hidden)]
pub unsafe fn as_mut<'a, T: ?Sized>(v: *const T) -> &'a mut T {
    &mut *(v as *mut T)
}

pub fn as_slice<T: ?Sized>(x: &T) -> &[u8] {
    let ptr: *const T = x;
    let ptr: *const u8 = ptr as *const u8;
    unsafe {
        std::slice::from_raw_parts(ptr, std::mem::size_of_val(x))
    }
}

pub fn as_slice64<T: ?Sized>(x: &T) -> &[u64] {
    let len = std::mem::size_of_val(x);
    assert_eq!(len % 8, 0, "Cannot convert an object of size {} bytes to [u64]", len);
    let ptr: *const T = x;
    let ptr: *const u64 = ptr as *const u64;
    unsafe {
        std::slice::from_raw_parts(ptr, len/8)
    }
}

#[inline(always)]
pub unsafe fn read<'a, T: ?Sized>(raw: *mut u8) -> &'a mut T {
    assert_ne!(raw, std::ptr::null_mut(), "null dereferencing");
    union U<T: ?Sized> {
        raw: *mut u8,
        rf: *mut T,
    }
    &mut *U { raw }.rf
}

#[inline(always)]
pub unsafe fn read_addr<'a, T: ?Sized>(addr: u64) -> &'a mut T {
    assert_ne!(addr, u64::MAX, "null dereferencing");
    union U<T: ?Sized> {
        addr: u64,
        rf: *mut T,
    }
    &mut *U { addr }.rf
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
            "too many slots are used (len = {})", N
        );

        self.data[self.tail] = x;
        self.tail = (self.tail + 1) % N;
    }

    #[inline]
    pub fn push_sync(&mut self, x: T) {
        debug_assert!(
            (self.tail+1)%N != self.head,
            "too many slots are used (len = {})", N
        );
        self.data[self.tail] = x;

        #[cfg(not(feature = "no_flush_alloc"))]
        persist(&self.data[self.tail], 8, false);
        
        self.tail = (self.tail + 1) % N;

        #[cfg(not(feature = "no_flush_alloc"))]
        persist(&self.head, 16, false);
    }

    #[inline]
    pub fn sync_all(&self) {
        if self.head == self.tail {
            #[cfg(not(feature = "no_flush_alloc"))]
            persist(&self.head, 16, false);
            return;
        }
        #[cfg(not(feature = "no_flush_alloc"))]
        {
            let h = &self.data[self.head] as *const _ as usize;
            let t = &self.data[self.tail] as *const _ as usize;
            if h < t {
                persist(&self.data[self.head], t - h, false);
                persist(&self.head, 16, false);
            } else {
                let b = self as *const Self as usize;
                persist(self, h - b, false);
                let b = b + std::mem::size_of::<Self>();
                persist(&self.data[self.tail], b - t, false);
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
    pub fn foreach<F: FnMut(T) -> ()>(&self, mut f: F) {
        let mut head = self.head;
        while head != self.tail {
            f(self.data[head]);
            head = (head + 1) % N;
        }
    }

    #[inline]
    pub fn drain_atomic<F: FnMut(T), E: Fn()>(&mut self, mut f: F, end: E) {
        while self.head != self.tail {
            f(self.data[self.head]);
            self.head = (self.head + 1) % N;
            end();
        }
    }

    #[inline]
    pub fn foreach_reverse<F: FnMut(T) -> ()>(&self, mut f: F) {
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

pub struct SpinLock {
    lock: *mut u8
}

impl SpinLock {
    pub fn acquire(lock: *mut u8) -> Self {
        unsafe { while std::intrinsics::atomic_cxchg_acqrel(lock, 0, 1).0 == 1 {} }
        Self { lock }
    }
}

impl Drop for SpinLock {
    fn drop(&mut self) {
        unsafe { std::intrinsics::atomic_store_rel(self.lock, 0); }
    }
}

#[cfg(feature = "verbose")]
pub static VERBOSE: crate::cell::LazyCell<bool> = crate::cell::LazyCell::new(||
    if let Ok(val) = std::env::var("VERBOSE") {
        val == "1"
    } else {
        false
    });

#[macro_export]
macro_rules! log {
    ($p:tt, $c:tt, $tag:expr, $msg:expr, $($args:tt)*) => {
        #[cfg(feature = "verbose")] {
            use term_painter::Color::*;
            use term_painter::ToStyle;

            if *$crate::utils::VERBOSE {
                println!("{:<8} {}", $p::name().to_owned() + ":",
                    $c.paint(format!("{:>10}  {}", $tag, format!($msg, $($args)*))));
            }
        }
    };
    (@none, $c:tt, $tag:expr, $msg:expr, $($args:tt)*) => {
        #[cfg(feature = "verbose")] {
            use term_painter::Color::*;
            use term_painter::ToStyle;

            if *$crate::utils::VERBOSE {
                println!("{:<8} {}", "",
                    $c.paint(format!("{:>10}  {}", $tag, format!($msg, $($args)*))));
            }
        }
    };
}

pub const fn nearest_pow2(mut v: u64) -> u64 {
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v |= v >> 32;
    v += 1;
    v
}