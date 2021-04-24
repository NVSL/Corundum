extern crate num;
use num::complex::Complex;
use std::f64::consts::PI;

const I: Complex<f64> = Complex { re: 0.0, im: 1.0 };

pub fn read(filename: &str) -> std::io::Result<Vec<Complex<f64>>> {
    use std::fs::File;
    use std::io::prelude::*;
    use std::io::BufReader;

    let file = File::open(filename)?;
    let mut buf_reader = BufReader::new(file);
    let mut buf = String::new();
    let mut res = vec![];
    while let Ok(s) = buf_reader.read_line(&mut buf) {
        if s > 0 {
            buf.pop();
            let n: Vec<&str> = buf.split(' ').collect();
            res.push(Complex::new(n[0].parse().unwrap(), n[1].parse().unwrap()));
            buf.clear();
        } else {
            break;
        }
    }
    Ok(res)
}

pub fn fft(filename: &str) -> Vec<Complex<f64>> {
    let input = read(filename).unwrap();

    // round n (length) up to a power of 2:
    let n_orig = input.len();
    let n = n_orig.next_power_of_two();
    // copy the input into a buffer:
    let mut buf_a = input.to_vec();
    // right pad with zeros to a power of two:
    buf_a.append(&mut vec![Complex { re: 0.0, im: 0.0 }; n - n_orig]);
    // alternate between buf_a and buf_b to avoid allocating a new vector each time:
    let mut buf_b = buf_a.clone();

    fn ft(a: &[Complex<f64>], c: &mut [Complex<f64>], n: usize, is: usize) {
        for k in 0..n {
            let mut s = Complex::new(0.0, 0.0);
            for x in 0..n {
                s += (I * PI * 2.0 * (is as f64) / (n as f64)).exp() * a[x];
            }
            c[k] = s;
        }
    }

    ft(&buf_a, &mut buf_b, n, 1);

    buf_b
}

fn show(label: &str, buf: &[Complex<f64>]) {
    println!("{}", label);
    let string = buf
        .into_iter()
        .map(|x| format!("{:.4}{:+.4}i", x.re, x.im))
        .collect::<Vec<_>>()
        .join(", ");
    println!("{}", string);
}

pub fn fft_persistent(filename: &str) -> Vec<Complex<f64>> {
    use corundum::default::*;

    type P = BuddyAlloc;

    struct FFT {
        a: PRefCell<PVec<PCell<Complex<f64>>>>,
        c: PRefCell<PVec<PCell<Complex<f64>>>>,
        n: PCell<usize>,
        is: PCell<usize>,
        k: PCell<usize>,
        x: PCell<usize>,
        s: PCell<Complex<f64>>,
        filled: PCell<bool>,
    }

    impl RootObj<P> for FFT {
        fn init(_j: &Journal) -> Self {
            Self {
                a: PRefCell::new(PVec::new()),
                c: PRefCell::new(PVec::new()),
                n: PCell::new(0),
                is: PCell::new(0),
                k: PCell::new(0),
                x: PCell::new(0),
                s: PCell::new(Complex::new(0.0, 0.0)),
                filled: PCell::new(false),
            }
        }
    }

    impl FFT {
        pub fn fill(&self, input: &[Complex<f64>], n: usize, step: usize) {
            P::transaction(|j| {
                let mut buf_a = self.a.borrow_mut(j);
                let mut buf_b = self.c.borrow_mut(j);
                buf_a.reserve(input.len(), j);
                buf_b.reserve(input.len(), j);
                for c in input {
                    buf_a.push(PCell::new(*c), j);
                    buf_b.push(PCell::new(*c), j);
                }
                self.n.set(n, j);
                self.is.set(step, j);
                self.k.set(0, j);
                self.x.set(0, j);
                self.filled.set(true, j);
            })
            .unwrap();
        }

        /// Performs one iteration of the following fourier transfer implementation
        ///
        /// ```
        /// fn ft(a: &[Complex<f64>], c: &mut [Complex<f64>], n: usize, is: usize) {
        ///     for k in 0..n {
        ///         let mut s = Complex::new(0.0, 0.0);
        ///         for x in 0..n {
        ///             s += (I * PI * 2.0 * (is as f64) / (n as f64)).exp() * a[x];
        ///         }
        ///         c[k] = s;
        ///     }
        /// }
        /// ```
        fn process(&self) -> bool {
            P::transaction(|j| {
                let n = self.n.get();
                let k = self.k.get();
                let x = self.x.get();
                if x == n {
                    let c = self.c.borrow();
                    c[k].set(self.s.get(), j);
                    self.s.set(Complex::new(0.0, 0.0), j);
                    self.k.update(|x| x+1, j);
                    self.x.set(0, j);
                    return false;
                }
                if k == n {
                    return true;
                }
                let is = self.is.get();
                let a = self.a.borrow();
                self.s.set(
                    self.s.get() + (I * PI * 2.0 * (is as f64) / (n as f64)).exp() * a[x].get(),
                    j,
                );
                self.x.update(|x| x+1, j);

                false
            })
            .unwrap()
        }

        pub fn fft(&self, filename: &str) -> std::vec::Vec<Complex<f64>> {
            // fill the persistent object if it was not filled before
            if !self.filled.get() {
                let input = read(filename).unwrap();

                // round n (length) up to a power of 2:
                let n_orig = input.len();
                let n = n_orig.next_power_of_two();
                // copy the input into a buffer:
                let mut buf_a = input.to_vec();
                // right pad with zeros to a power of two:
                buf_a.append(&mut vec![Complex { re: 0.0, im: 0.0 }; n - n_orig]);
                self.fill(buf_a.as_slice(), n, 1);
            }

            while !self.process() {}

            let c = self.c.borrow();
            c.cast(|v| v.get())
        }
    }

    let root = P::open::<FFT>("fft.pool", O_CFNE).unwrap();

    root.fft(filename)
}

fn main() {
    use std::env;

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        if args[1] == "p" {
            let output = fft_persistent("fft.in");
            show("fft output:", &output);
            return;
        }
    }

    let output = fft("fft.in");
    show("fft output:", &output);
}
