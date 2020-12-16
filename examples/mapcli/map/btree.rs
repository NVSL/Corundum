//! btree.rs -- textbook implementation of btree /w preemptive splitting
//! equivalent to [btree example] from PMDK.
//!
//! [btree example]: https://github.com/pmem/pmdk/blob/master/src/examples/libpmemobj/tree_map/btree_map.c

use corundum::ptr::NonNull;
use std::panic::UnwindSafe;
use corundum::default::*;
use std::fmt::Debug;
use crate::map::*;

type PmemObj<T> = Prc<PRefCell<T>>;
type Ptr<T> = corundum::ptr::Ptr<T,P>;

const BTREE_ORDER: usize = 8;
const BTREE_MIN: usize = (BTREE_ORDER / 2) - 1;

#[derive(Copy, Clone, PClone, Default, Debug)]
pub struct NodeItem<V: PSafe> {
    key: u64,
    val: V,
}

#[derive(PClone, Default, Debug)]
pub struct Node<V: PSafe> {
    n: usize, // Number of occupied slots
    items: [NodeItem<V>; BTREE_ORDER - 1],
    slots: [Option<PmemObj<Node<V>>>; BTREE_ORDER],
}

pub struct BTree<V: PSafe> {
    root: PmemObj<Node<V>>,
}

impl<V: PSafe + Default> NodeItem<V> {
    #[inline]
    fn empty(&mut self) {
        self.key = 0;
        self.val = Default::default();
    }
}

impl<V: PSafe + Default + Copy> Node<V> {
    #[inline]
    fn insert_node(
        &mut self,
        p: &mut usize,
        item: NodeItem<V>,
        left: PmemObj<Node<V>>,
        right: PmemObj<Node<V>>,
        j: &Journal
    ) -> PmemObj<Node<V>> {
        let p = *p;
        if self.items[p].key != 0 {
            /* move all existing data */
            self.items[p..].rotate_right(1);
            self.slots[p..].rotate_right(1);
        }
        self.slots[p] = Some(left);
        self.slots[p + 1] = Some(right);
        self.insert_item_at(p, item);
        self.slots[p + 1].as_ref().unwrap().pclone(j)
    }

    #[inline]
    fn insert_item_at(&mut self, pos: usize, item: NodeItem<V>) {
        self.items[pos] = item;
        self.n += 1;
    }

    #[inline]
    fn clear_node(&mut self) {
        for n in &mut self.slots {
            *n = None;
        }
    }

    #[inline]
    fn split(&mut self, m: &mut NodeItem<V>) -> Self {
        let mut right = Self::default();
        let c = BTREE_ORDER / 2;
        *m = self.items[c - 1]; /* select median item */
        self.items[c - 1].empty();

        /* move everything right side of median to the new node */
        for i in c..BTREE_ORDER {
            if i != BTREE_ORDER - 1 {
                right.items[right.n] = self.items[i];
                right.n += 1;
                self.items[i].empty();
            }
            right.slots[i - c] = self.slots[i].take();
        }
        self.n = c - 1;
        right
    }

    #[inline]
    fn insert_item(&mut self, p: usize, item: NodeItem<V>) {
        if self.items[p].key != 0 {
            self.items[p..].rotate_right(1);
        }
        self.insert_item_at(p, item);
    }

    #[inline]
    fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
        for i in 0..self.n + 1 {
            if let Some(p) = &self.slots[i] {
                if p.borrow().foreach(f) {
                    return true;
                }
            }

            if i != self.n && self.items[i].key != 0 {
                if f(&self.items[i].key, &self.items[i].val) {
                    return true;
                }
            }
        }
        false
    }

    #[inline]
    unsafe fn lookup<'a>(slf: NonNull<Self>, key: u64) -> Option<&'a V> {
        for i in 0..slf.n {
            if i < slf.n {
                if slf.items[i].key == key {
                    let slf = slf.clone();
                    return Some(NonNull::new_unchecked(&slf.items[i].val).as_ref());
                } else {
                    if slf.items[i].key > key {
                        return if let Some(slot) = &slf.slots[i] {
                            Node::lookup(slot.as_non_null(), key)
                        } else {
                            None
                        };
                    }
                }
            } else {
                return if let Some(slot) = &slf.slots[i] {
                    Node::lookup(slot.as_non_null(), key)
                } else {
                    None
                };
            }
        }
        None
    }

    #[inline]
    fn remove(&mut self, p: usize) {
        self.items[p].empty();
        if self.n != 1 && p != BTREE_ORDER - 2 {
            self.items[p..].rotate_left(1);
        }
        self.n -= 1;
    }

    #[inline]
    fn rotate_right(
        &mut self,
        mut node: PNonNull<Node<V>>,
        mut parent: PNonNull<Node<V>>,
        p: usize
    ) {
        let n = node.n;
        node.insert_item(n, parent.items[p]);

        parent.items[p] = self.items[0];

        let n = node.n;
        node.slots[n] = self.slots[0].take();

        self.n -= 1;
        self.slots.rotate_left(1);
        self.items.rotate_left(1);
    }

    #[inline]
    fn rotate_left(
        &mut self,
        mut node: PNonNull<Node<V>>,
        mut parent: PNonNull<Node<V>>,
        p: usize
    ) {
        node.insert_item(0, parent.items[p - 1]);

        parent.items[p - 1] = self.items[self.n - 1];

        node.slots.rotate_right(1);
        node.slots[0] = self.slots[self.n].take();

        self.n -= 1;
    }
}

impl<V: PSafe + Default + Copy + PClone<P> + Debug> BTree<V>
where
    NodeItem<V>: PClone<P>,
{
    #[inline]
    fn find_dest_node_in(
        &self,
        nn: &PmemObj<Node<V>>,
        key: u64,
        p: &mut usize,
        j: &Journal,
    ) -> PNonNull<Node<V>> {
        let n = unsafe {nn.as_non_null_mut(j)};
        for i in 0..BTREE_ORDER - 1 {
            *p = i;

            /*
             * The key either fits somewhere in the middle or at the
             * right edge of the node.
             */
            if n.n == i || n.items[i].key > key {
                if let Some(slot) = &n.slots[i] {
                    return self.find_dest_node(slot, Some(n), key, p, j);
                } else {
                    return n;
                }
            }
        }

        if let Some(slot) = &n.slots[BTREE_ORDER - 1] {
            self.find_dest_node(slot, Some(n), key, p, j)
        } else {
            n
        }
    }

    #[inline]
    fn find_dest_node(
        &self,
        nn: &PmemObj<Node<V>>,
        parent: Option<PNonNull<Node<V>>>,
        key: u64,
        p: &mut usize,
        j: &Journal,
    ) -> PNonNull<Node<V>> {
        let mut n = unsafe {nn.as_non_null_mut(j)};
        if n.n == BTREE_ORDER - 1 {
            /* node is fullerform a split */
            let mut m = NodeItem::default();
            let right = n.split(&mut m);

            if let Some(mut parent) = parent {
                let right = Prc::new(PRefCell::new(right, j), j);
                let right = parent.insert_node(p, m, nn.pclone(j), right, j);
                if key > m.key {
                    /* select node to continue search */
                    self.find_dest_node_in(&right, key, p, j)
                } else {
                    self.find_dest_node_in(nn, key, p, j)
                }
            } else {
                /* replacing root node, the tree grows in height */
                let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
                let mut slots: [Option<PmemObj<Node<V>>>; BTREE_ORDER] = Default::default();
                items[0] = m;
                slots[0] = Some(Prc::new(PRefCell::new(n.pclone(j), j), j));
                slots[1] = Some(Prc::new(PRefCell::new(right, j), j));
                let up = Node {
                        n: 1,
                        items,
                        slots,
                    };
                let mut root = unsafe { self.root.as_non_null_mut(j) };
                *root = up;
                self.find_dest_node_in(&self.root, key, p, j)
            }
        } else {
            self.find_dest_node_in(nn, key, p, j)
        }
    }

    #[inline]
    fn insert_empty(&self, item: NodeItem<V>, j: &Journal) {
        let mut root = self.root.borrow_mut(j);
        let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
        items[0] = item;
        *root = Node {
            n: 1,
            items,
            slots: Default::default(),
        };
    }

    #[inline]
    fn get_leftmost_leaf(
        &self,
        n: PNonNull<Node<V>>,
        p: &mut PNonNull<Node<V>>,
        j: &Journal,
    ) -> PNonNull<Node<V>> {
        if let Some(slot) = &n.slots[0] {
            *p = n;
            self.get_leftmost_leaf(unsafe { slot.as_non_null_mut(j) }, p, j)
        } else {
            n
        }
    }

    #[inline]
    fn remove_from(&self, mut node: PNonNull<Node<V>>, p: usize, j: &Journal) {
        if node.slots[0].is_none() {
            /* leaf */
            node.remove(p);
        } else {
            let mut lp = node;
            let mut same = false;
            let lm = if let Some(slot) = &node.slots[p + 1] {
                self.get_leftmost_leaf(unsafe { slot.as_non_null_mut(j) }, &mut lp, j)
            } else {
                same = true;
                node
            };
            node.items[p] = lm.items[0];
            self.remove_from(lm, 0, j);

            if lm.n < BTREE_MIN {
                self.rebalance(lm, lp, if same { p + 1 } else { 0 }, j);
            }
        }
    }

    fn remove_item(
        &self,
        node: PNonNull<Node<V>>,
        parent: Option<PNonNull<Node<V>>>,
        key: u64,
        p: usize,
        j: &Journal,
    ) -> Option<V> {
        let mut ret = None;
        {
            let node = node;
            for i in 0..node.n + 1 {
                if i != node.n && node.items[i].key == key {
                    ret = Some(node.items[i].val);
                    self.remove_from(node, i, j);
                    break;
                } else if let Some(slot) = &node.slots[i] {
                    if i == node.n || node.items[i].key > key {
                        ret = self.remove_item(unsafe { slot.as_non_null_mut(j) }, Some(node), key, i, j);
                        break;
                    }
                }
            }
        }

        if let Some(parent) = parent {
            if node.n < BTREE_MIN {
                self.rebalance(node, parent, p, j);
            }
        }

        ret
    }

    fn rebalance(
        &self,
        node: PNonNull<Node<V>>,
        parent: PNonNull<Node<V>>,
        p: usize,
        j: &Journal,
    ) {
        let mut rsb = if p >= parent.n {
            None
        } else {
            if let Some(slot) = &parent.slots[p + 1] {
                Some(unsafe { slot.as_non_null_mut(j) })
            } else {
                None
            }
        };
        let mut lsb = if p == 0 {
            None
        } else {
            if let Some(slot) = &parent.slots[p - 1] {
                Some(unsafe { slot.as_non_null_mut(j) })
            } else {
                None
            }
        };
        if let Some(rsb) = rsb.as_mut() {
            if rsb.n > BTREE_MIN {
                rsb.rotate_right(node, parent, p);
                return;
            }
        }
        if let Some(lsb) = lsb.as_mut() {
            if lsb.n > BTREE_MIN {
                lsb.rotate_left(node, parent, p);
                return;
            }
        }
        if let Some(rsb) = rsb.as_ref() {
            self.merge(*rsb, node, parent, p, j)
        } else if let Some(lsb) = lsb.as_ref() {
            self.merge(node, *lsb, parent , p - 1, j)
        }
    }

    #[inline]
    fn merge(
        &self,
        mut rn: PNonNull<Node<V>>,
        mut node: PNonNull<Node<V>>,
        mut parent: PNonNull<Node<V>>,
        p: usize,
        j: &Journal,
    ) {
        let n = node.n;
        node.items[n] = parent.items[p];
        node.n += 1;

        let n = node.n;
        node.items[n..n + rn.n].copy_from_slice(&rn.items[0..rn.n]);
        for i in 0..rn.n + 1 {
            node.slots[n + i] = rn.slots[i].take();
        }
        node.n += rn.n;
        rn.n = 0;

        parent.n -= 1;
        parent.items[p..].rotate_left(1);
        parent.slots[p + 1..].rotate_left(1);
        parent.items.last_mut().unwrap().empty();
        *parent.slots.last_mut().unwrap() = None;

        if parent.n == 0 {
            let mut root = self.root.borrow_mut(j);
            *root = node.pclone(j);
        }
    }
}

impl<V: PSafe + Default + PClone<P> + Debug> Map<u64, V> for BTree<V>
where
    NodeItem<V>: Clone,
    V: std::panic::RefUnwindSafe + TxInSafe + UnwindSafe + Copy,
    Self: 'static
{
    fn clear(&self) {
        P::transaction(|j| {
            let mut root = self.root.borrow_mut(j);
            *root = Node::default();
        })
        .unwrap();
    }

    fn insert(&self, key: u64, val: V) {
        P::transaction(|j| {
            let item = NodeItem::<V> { key, val };
            if self.is_empty() {
                self.insert_empty(item, j);
            } else {
                let mut p = 0;
                let mut dest = self.find_dest_node(&self.root, None, key, &mut p, j);
                dest.insert_item(p, item);
            }
        })
        .unwrap();
    }

    fn remove(&self, key: u64) {
        P::transaction(|j| {
            let root = unsafe {self.root.as_non_null_mut(j)};
            self.remove_item(root, None, key, 0, j);
        })
        .unwrap();
    }

    fn is_empty(&self) -> bool {
        self.root.borrow().n == 0
    }

    fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
        self.root.borrow().foreach(f)
    }

    fn lookup(&self, key: u64) -> Option<&V> {
        unsafe { Node::lookup(self.root.as_non_null(), key) }
    }
}

impl<V: PSafe + Default> RootObj<P> for BTree<V> {
    fn init(j: &Journal) -> Self {
        BTree {
            root: Prc::new(PRefCell::new(Default::default(), j), j),
        }
    }
}