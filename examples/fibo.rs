use corundum::alloc::*;
use corundum::cell::*;
use corundum::stm::*;

corundum::pool!(pool1);
corundum::pool!(pool2);

type P1 = pool1::BuddyAlloc;
type P2 = pool2::BuddyAlloc;

struct Root<P: MemPool> {
    val: PRefCell<f64, P>,
    idx: PRefCell<u64, P>,
}
impl<M: MemPool> Default for Root<M> {
    fn default() -> Self {
        Root {
            val: PRefCell::new(0.0),
            idx: PRefCell::new(0),
        }
    }
}

fn main() {
    let n1 = P1::open::<Root<P1>>("fibo1.pool", O_CFNE).unwrap();
    let n2 = P2::open::<Root<P2>>("fibo2.pool", O_CFNE).unwrap();

    while !Chaperon::session("fibo.pool", || {
        let n1_val = 1f64.max(*n1.val.borrow());
        let n1_idx = *n1.idx.borrow();

        if n1_idx >= 100 {
            return true;
        }

        let n2_val = P2::transaction(|j| {
            let mut n2 = n2.val.borrow_mut(j);
            let old_n2 = *n2;
            *n2 = n1_val;
            old_n2
        }).unwrap();

        P1::transaction(|j| {
            let mut n1_idx = n1.idx.borrow_mut(j);
            let mut n1 = n1.val.borrow_mut(j);
            *n1 += n2_val;
            *n1_idx += 1;
        }).unwrap();

        false
    }).unwrap() {}

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
