// //! btree.rs -- textbook implementation of btree /w preemptive splitting
// //! equivalent to [btree example] from PMDK.
// //!
// //! [btree example]: https://github.com/pmem/pmdk/blob/master/src/examples/libpmemobj/tree_map/btree_map.c

// use std::fmt::Debug;
// use std::panic::UnwindSafe;
// use crndm::stm::Journal;
// use crndm::*;
// use crndm::cell::*;
// use crndm::alloc::*;
// use crndm::sync::{Parc,Mutex};
// use crndm::clone::Clone;
// use std::clone::Clone as StdClone;
// use crate::map::*;

// type PmemObj<T> = Option<Parc<Mutex<T,P>,P>>;

// const BTREE_ORDER: usize = 8;
// const BTREE_MIN: usize = (BTREE_ORDER / 2) - 1;

// #[derive(Copy,Debug)]
// pub struct NodeItem<V: PSafe> {
//     key: u64,
//     val: V
// }

// impl<V: PSafe + Copy> StdClone for NodeItem<V> {
//     fn clone(&self) -> Self {
//         Self {
//             key: self.key,
//             val: self.val
//         }
//     }
// }

// #[derive(Debug)]
// pub struct Node<V: PSafe> {
//     n: usize, // Number of occupied slots
//     items: [NodeItem<V>; BTREE_ORDER - 1],
//     slots: [PmemObj<Node<V>>; BTREE_ORDER]
// }

// impl<V: PSafe + Copy> PClone<P> for Node<V> {

//     #[inline]
//     fn pclone(&self, j: &Journal<P>) -> Self {
//         let mut slots:[PmemObj<Node<V>>; BTREE_ORDER] = Default::default();
//         for i in 0..BTREE_ORDER {
//             if let Some(slot) = &self.slots[i] {
//                 slots[i] = Some(slot.pclone(j));
//             }
//         }
//         Self {
//             n: self.n,
//             items: self.items,
//             slots
//         }
//     }
// }

// pub struct PBTree<V: PSafe> {
//     root: Mutex<PmemObj<Node<V>>,P>
// }

// impl<V: PSafe + Default > Default for NodeItem<V> {
//     fn default() -> Self {
//         Self { key: 0, val: Default::default() }
//     }
// }

// impl<V: PSafe + Default> NodeItem<V> {

//     #[inline]
//     fn empty(&mut self) {
//         self.key = 0;
//         self.val = Default::default();
//     }
// }

// // impl<V: PSafe + Default + Copy> Node<V> {

// //     #[inline]
// //     fn new() -> Self {
// //         Self {
// //             n: 0,
// //             items: Default::default(),
// //             slots: Default::default()
// //         }
// //     }

// //     #[inline]
// //     fn one(item: NodeItem<V>, j: &Journal<P>) -> PmemObj<Self> {
// //         let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
// //         items[0] = item;
// //         Some(Parc::new(Mutex::new(Self {
// //             n: 1,
// //             items,
// //             slots: Default::default()
// //         },j),j))
// //     }

// //     #[inline]
// //     fn empty(j: &Journal<P>) -> PmemObj<Self> {
// //         Some(Parc::new(Mutex::new(Self {
// //             n: 0,
// //             items: Default::default(),
// //             slots: Default::default()
// //         },j),j))
// //     }

// //     #[inline]
// //     fn empty2(j: &Journal<P>) -> Parc<Mutex<Self,P>,P> {
// //         Parc::new(Mutex::new(Self {
// //             n: 0,
// //             items: Default::default(),
// //             slots: Default::default()
// //         },j),j)
// //     }

// //     #[inline]
// //     fn insert_node(&mut self, p: &mut usize, item: NodeItem<V>,
// //         left: Parc<Mutex<Node<V>,P>,P>,
// //         right: Parc<Mutex<Node<V>,P>,P>
// //     ) {
// //         let p = *p;
// //         if self.items[p].key != 0 { /* move all existing data */
// //             self.items[p..].rotate_right(1);
// //             self.slots[p..].rotate_right(1);
// //         }
// //         self.slots[p] = Some(left);
// //         self.slots[p+1] = Some(right);
// //         self.insert_item_at(p, item);
// //     }

// //     #[inline]
// //     fn insert_item_at(&mut self, pos: usize, item: NodeItem<V>) {
// //         self.items[pos] = item;
// //         self.n += 1;
// //     }

// //     #[inline]
// //     fn clear_node(&mut self) {
// //         for n in &mut self.slots {
// //             *n = None;
// //         }
// //     }

// //     #[inline]
// //     fn split(&mut self, m: &mut NodeItem<V>) -> Self {
// //         let mut right = Node::<V>::new();
// //         let c = BTREE_ORDER / 2;
// //         *m = self.items[c-1]; /* select median item */
// //         self.items[c-1].empty();

// //         /* move everything right side of median to the new node */
// //         for i in c..BTREE_ORDER {
// //             if i != BTREE_ORDER - 1 {
// //                 let n = right.n;
// //                 right.items[n] = self.items[i];
// //                 right.n += 1;
// //                 self.items[i].empty();
// //             }
// //             right.slots[i - c].swap(self.slots[i].swap(None));
// //         }
// //         self.n = c - 1;
// //         right
// //     }

// //     #[inline]
// //     fn insert_item(&mut self, p: usize, item: NodeItem<V>) {
// //         if self.items[p].key != 0 {
// //             self.items[p..].rotate_right(1);
// //         }
// //         self.insert_item_at(p, item);
// //     }

// //     #[inline]
// //     fn foreach<F: Copy + Fn(u64, V)->bool>(&self, f: F, j: &Journal<P>) -> bool {
// //         for i in 0 .. self.n+1 {
// //             if let Some(p) = &self.slots[i] {
// //                 let p = p.lock(j);
// //                 if p.foreach(f, j) {
// //                     return true;
// //                 }
// //             }

// //             if i != self.n && self.items[i].key != 0 {
// //                 if f(self.items[i].key, self.items[i].val) {
// //                     return true;
// //                 }
// //             }
// //         }
// //         false
// //     }

// //     #[inline]
// //     fn lookup(&self, key: u64) -> Option<V> {
// //         for i in 0..self.n {
// //             if i < self.n {
// //                 if self.items[i].key == key {
// //                     return Some(self.items[i].val)
// //                 } else {
// //                     if self.items[i].key > key {
// //                         return if let Some(slot) = &self.slots[i] {
// //                             slot.lookup(key)
// //                         } else {
// //                             None
// //                         }
// //                     }
// //                 }
// //             } else {
// //                 return if let Some(slot) = &self.slots[i] {
// //                     slot.lookup(key)
// //                 } else {
// //                     None
// //                 }
// //             }
// //         }
// //         None
// //     }

// //     #[inline]
// //     fn remove(&mut self, p: usize) {
// //         self.items[p].empty();
// //         if self.n != 1 && p != BTREE_ORDER - 2 {
// //             self.items[p..].rotate_left(1);
// //         }
// //         self.n -= 1;
// //     }

// //     #[inline]
// //     fn rotate_right(&mut self,
// //         node: &Parc<Mutex<Node<V>,P>,P>,
// //         parent: &Parc<Mutex<Node<V>,P>,P>,
// //         p: usize,
// //         j: &Journal<P>
// //     ) {
// //         let sep = parent.items[p];
// //         let mut node = node.borrow_mut(j);
// //         let n = node.n;
// //         node.insert_item(n, sep);

// //         let mut parent = parent.borrow_mut(j);
// //         parent.items[p] = self.items[0];

// //         let n = node.n;
// //         node.slots[n] = self.slots[0].pclone(j);

// //         self.n -= 1;
// //         self.slots[0] = None;
// //         self.slots.rotate_left(1);
// //         self.items.rotate_left(1);
// //     }

// //     #[inline]
// //     fn rotate_left(&mut self,
// //         node: &Parc<Mutex<Node<V>,P>,P>,
// //         parent: &Parc<Mutex<Node<V>,P>,P>,
// //         p: usize,
// //         j: &Journal<P>
// //     ) {
// //         let sep = parent.items[p - 1];
// //         let mut node = node.borrow_mut(j);
// //         node.insert_item(0, sep);

// //         let mut parent = parent.borrow_mut(j);
// //         parent.items[p - 1] = self.items[self.n - 1];

// //         node.slots.rotate_right(1);
// //         node.slots[0] = self.slots[self.n].pclone(j);

// //         self.n -= 1;
// //         self.slots[self.n - 1] = None;
// //     }
// // }

// // impl<V: PSafe + Default + Copy + Debug> PBTree<V> where NodeItem<V>: StdClone {

// //     #[inline]
// //     fn find_dest_node_in<'a>(&'a self, n: &Parc<Mutex<Node<V>,P>,P>,
// //         key: u64, p: &mut usize, j: &'a Journal<P>
// //     ) -> Parc<Mutex<Node<V>,P>,P> {
// //         for i in 0..BTREE_ORDER-1 {
// //             *p = i;

// //             /*
// //             * The key either fits somewhere in the middle or at the
// //             * right edge of the node.
// //             */
// //             if n.n == i || n.items[i].key > key {
// //                 if let Some(slot) = &n.slots[i] {
// //                     return self.find_dest_node(slot, Some(&n), key, p, j);
// //                 } else {
// //                     return n.pclone(j)
// //                 }
// //             }
// //         }

// //         if let Some(slot) = &n.slots[BTREE_ORDER-1] {
// //             self.find_dest_node(slot, Some(&n), key, p, j)
// //         } else {
// //             n.pclone(j)
// //         }
// //     }

// //     #[inline]
// //     fn find_dest_node<'a>(&'a self, n: &Parc<Mutex<Node<V>,P>, P>,
// //         parent: Option<&Parc<Mutex<Node<V>,P>, P>>, key: u64, p: &mut usize, j: &'a Journal<P>
// //     ) -> Parc<Mutex<Node<V>,P>,P> {
// //         if n.n == BTREE_ORDER - 1 { /* node is full, perform a split */
// //             let mut m = NodeItem::default();
// //             let right = n.borrow_mut(j).split(&mut m);

// //             let right = Parc::new(Mutex::new(right,j),j);
// //             if let Some(parent) = parent {
// //                 if key > m.key { /* select node to continue search */
// //                     let mut parent = parent.borrow_mut(j);
// //                     parent.insert_node(p, m, n.pclone(j), right.pclone(j));
// //                     self.find_dest_node_in(&right, key, p, j)
// //                 } else {
// //                     self.find_dest_node_in(n, key, p, j)
// //                 }
// //             } else { /* replacing root node, the tree grows in height */
// //                 let mut up = Node::<V>::new();
// //                 up.n = 1;
// //                 up.items[0] = m;
// //                 up.slots[0] = Some(n.pclone(j));
// //                 up.slots[1] = Some(right);
// //                 let mut root = self.root.borrow_mut(j);
// //                 *root = Some(Parc::new(Mutex::new(up,j),j));

// //                 if let Some(root) = &*root {
// //                     self.find_dest_node_in(root, key, p, j)
// //                 } else {
// //                     n.pclone(j)
// //                 }
// //             }
// //         } else {
// //             self.find_dest_node_in(n, key, p, j)
// //         }
// //     }

// //     #[inline]
// //     fn insert_empty(&self, item: NodeItem<V>, j: &Journal<P>) {
// //         let mut root = self.root.borrow_mut(j);
// //         *root = Node::one(item, j);
// //     }

// //     #[inline]
// //     fn get_leftmost_leaf<'a>(&self,
// //         n: &'a Parc<Mutex<Node<V>,P>,P>,
// //         ref mut p: &'a Parc<Mutex<Node<V>,P>,P>
// //     ) -> &'a Parc<Mutex<Node<V>,P>,P> {
// //         if let Some(slot) = &n.slots[0] {
// //             *p = n;
// //             self.get_leftmost_leaf(&slot, p)
// //         } else {
// //             n
// //         }
// //     }

// //     #[inline]
// //     fn remove_from(&self, node: &Parc<Mutex<Node<V>,P>,P>,
// //         p: usize, j: &Journal<P>
// //     ) {
// //         if node.slots[0].is_none() { /* leaf */
// //             let mut n = node.borrow_mut(j);
// //             n.remove(p);
// //         } else {
// //             let lp = node;
// //             let lm = if let Some(slot) = &node.slots[p + 1] {
// //                 self.get_leftmost_leaf(&slot, &lp)
// //             } else {
// //                 node
// //             };
// //             node.borrow_mut(j).items[p] = lm.items[0];
// //             self.remove_from(lm, 0, j);

// //             if lm.n < BTREE_MIN {
// //                 self.rebalance(lm, lp,
// //                     if lp as *const _ == node as *const _
// //                     { p + 1 } else { 0 }, j);
// //             }
// //         }
// //     }

// //     fn remove_item(&self, node: &Parc<Mutex<Node<V>,P>, P>,
// //         parent: Option<&Parc<Mutex<Node<V>,P>, P>>, key: u64, p: usize, j: &Journal<P>
// //     ) -> Option<V> {
// //         let mut ret = None;
// //         for i in 0..node.n+1 {
// //             if i != node.n && node.items[i].key == key {
// //                 ret = Some(node.items[i].val);
// //                 self.remove_from(node, i, j);
// //                 break;
// //             } else if let Some(slot) = &node.slots[i] {
// //                 if i == node.n || node.items[i].key > key {
// //                     ret = self.remove_item(slot, Some(node), key, i, j);
// //                     break;
// //                 }
// //             }
// //         }

// //         if let Some(parent) = parent {
// //             if node.n < BTREE_MIN {
// //                 self.rebalance(node, parent, p, j);
// //             }
// //         }

// //         ret
// //     }

// //     fn rebalance(&self,
// //         node: &Parc<Mutex<Node<V>,P>,P>,
// //         parent: &Parc<Mutex<Node<V>,P>,P>,
// //         p: usize, j: &Journal<P>
// //     ) {
// //         let rsb = if p >= parent.n { &None } else {
// //             &parent.slots[p + 1]
// //         };
// //         let lsb = if p == 0 { &None } else {
// //             &parent.slots[p - 1]
// //         };
// //         if let Some(rsb) = rsb {
// //             if rsb.n > BTREE_MIN {
// //                 let mut rsb = rsb.borrow_mut(j);
// //                 rsb.rotate_right(node, parent, p, j);
// //                 return;
// //             }
// //         }
// //         if let Some(lsb) = lsb {
// //             if lsb.n > BTREE_MIN {
// //                 let mut lsb = lsb.borrow_mut(j);
// //                 lsb.rotate_left(node, parent, p, j);
// //                 return;
// //             }
// //         }
// //         if rsb.is_none() {
// //             if let Some(lsb) = lsb {
// //                 self.merge(node, lsb, parent, p - 1, j)
// //             }
// //         } else {
// //             if let Some(rsb) = rsb {
// //                 self.merge(rsb, node, parent, p, j)
// //             }
// //         }
// //     }

// //     #[inline]
// //     fn merge(&self,
// //         rn: &Parc<Mutex<Node<V>,P>,P>,
// //         node: &Parc<Mutex<Node<V>,P>,P>,
// //         parent: &Parc<Mutex<Node<V>,P>,P>,
// //         p: usize,
// //         j: &Journal<P>
// //     ) {
// //         let sep = parent.items[p];

// //         {
// //             let mut node = node.borrow_mut(j);
// //             let n = node.n;
// //             node.items[n] = sep;
// //             node.n += 1;

// //             let n = node.n;
// //             node.items[n..n+rn.n].copy_from_slice(&rn.items[0..rn.n]);
// //             for i in 0..rn.n+1 {
// //                 node.slots[n+i] = rn.slots[i].pclone(j);
// //             }
// //             node.n += rn.n;
// //             let mut parent = parent.borrow_mut(j);
// //             parent.n -= 1;
// //             parent.items[p..].rotate_left(1);
// //             parent.slots[p+1..].rotate_left(1);
// //             parent.items.last_mut().unwrap().empty();
// //             *parent.slots.last_mut().unwrap() = None;
// //         }

// //         if parent.n == 0 {
// //             let mut root = self.root.borrow_mut(j);
// //             *root = Some(node.pclone(j));
// //         }
// //     }
// // }

// impl<V: PSafe + Default + Debug> Map<u64,V> for PBTree<V> where
// NodeItem<V>: StdClone,
// V: TxInSafe + UnwindSafe + Copy {

//     fn clear(&self) {
//         // P::transaction(|j| {
//         //     if let Some(root) = &mut *self.root.borrow_mut(j) {
//         //         *root = Node::empty(j).unwrap();
//         //     }
//         // }).unwrap();
//     }

//     fn insert(&self, key: u64, val: V) {
//         // P::transaction(move |j| {
//         //     let item = NodeItem::<V> { key, val };
//         //     if self.is_empty() {
//         //         self.insert_empty(item, j);
//         //     } else {
//         //         let mut p = 0;
//         //         if let Some(root) = &*self.root.borrow() {
//         //             let dest = self.find_dest_node(root, None, key, &mut p, j);
//         //             let mut dest = dest.borrow_mut(j);
//         //             dest.insert_item(p, item);
//         //         }
//         //     }
//         // }).unwrap();
//     }

//     fn remove(&self, key: u64) {
//         // P::transaction(move |j| {
//         //     if let Some(root) = &*self.root.borrow() {
//         //         self.remove_item(root, None, key, 0, j);
//         //     }
//         // }).unwrap();
//     }

//     fn is_empty(&self) -> bool {
//         // if let Some(root) = &*self.root {
//         //     root.n == 0
//         // } else {
//         //     true
//         // }
//         true
//     }

//     fn foreach<F: Copy + Fn(u64, V)->bool>(&self, f: F) -> bool {
//         // if let Some(root) = &*self.root {
//         //     root.foreach(f)
//         // } else {
//         //     false
//         // }
//         false
//     }

//     fn lookup(&self, key: u64) -> Option<V> {
//         // if let Some(root) = &*self.root {
//         //     root.lookup(key)
//         // } else {
//         //     None
//         // }
//         None
//     }
// }

// impl<V: PSafe> RootObj<P> for PBTree<V> {
//     fn init(j: &Journal<P>) -> Self {
//         PBTree { root: Mutex::new(None, j) }
//     }
// }
