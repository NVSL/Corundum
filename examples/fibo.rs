use crndm::alloc::*;
use crndm::cell::*;
use crndm::stm::*;

crndm::pool!(pool1);
crndm::pool!(pool2);

type P1 = pool1::BuddyAlloc;
type P2 = pool2::BuddyAlloc;

struct Root<P: MemPool> {
    val: LogRefCell<f64, P>,
    idx: LogRefCell<u64, P>,
}
impl<M: MemPool> RootObj<M> for Root<M> {
    fn init(j: &Journal<M>) -> Self {
        Root {
            val: LogRefCell::new(0.0, j),
            idx: LogRefCell::new(0, j),
        }
    }
}

fn main() {
    let n1 = P1::open::<Root<P1>>("fibo1.pool", O_CFNE).unwrap();
    let n2 = P2::open::<Root<P2>>("fibo2.pool", O_CFNE).unwrap();

    while !Chaperon::session("fibo.pool", || {
        let n1_val = f64::max(1.0, *n1.val.borrow());
        let n1_idx = *n1.idx.borrow();

        if n1_idx >= 100 {
            return true;
        }

        let n2_val = P2::transaction(|j| {
            let mut n2 = n2.val.borrow_mut(j);
            let old_n2 = *n2;
            *n2 = n1_val;
            old_n2
        })
        .unwrap();

        P1::transaction(|j| {
            let mut n1_idx = n1.idx.borrow_mut(j);
            let mut n1 = n1.val.borrow_mut(j);
            *n1 += n2_val;
            *n1_idx += 1;
        })
        .unwrap();

        false
    })
    .unwrap()
    {}

    let n1 = n1.val.read();
    let n2 = n2.val.read();
    println!("n1 = {} (usage = {})", n1, P1::used());
    println!("n2 = {} (usage = {})", n2, P2::used());
    assert_eq!((n2, n1), calc(1.0, 1.0, n1));

    fn calc(n1: f64, n2: f64, stop: f64) -> (f64, f64) {
        if n1 >= stop {
            return (n2, n1);
        } else {
            calc(n1 + n2, n1, stop)
        }
    }
}
