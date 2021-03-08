// //! btree.rs -- textbook implementation of btree /w preemptive splitting
// //! equivalent to [btree example] from PMDK.
// //!
// //! [btree example]: https://github.com/pmem/pmdk/blob/master/src/examples/libpmemobj/tree_map/btree_map.c

// use crate::map::*;
// use std::fmt::Debug;
// use std::panic::UnwindSafe;

// type PmemObj<T> = Option<Rc<T>>;

// const BTREE_ORDER: usize = 8;
// const BTREE_MIN: usize = (BTREE_ORDER / 2) - 1;

// #[derive(Copy, Debug)]
// pub struct NodeItem<V> {
//     key: u64,
//     val: V,
// }

// impl<V: Copy> Clone for NodeItem<V> {
//     fn clone(&self) -> Self {
//         Self {
//             key: self.key,
//             val: self.val,
//         }
//     }
// }

// type PRefCell<T> = std::cell::RefCell<T>;

// #[derive(Debug)]
// pub struct Node<V> {
//     n: PRefCell<usize>, // Number of occupied slots
//     items: PRefCell<[NodeItem<V>; BTREE_ORDER - 1]>,
//     slots: PRefCell<[PmemObj<Node<V>>; BTREE_ORDER]>,
// }

// impl<V: Copy> Clone for Node<V> {
//     #[inline]
//     fn clone(&self) -> Self {
//         let mut slots: [PmemObj<Node<V>>; BTREE_ORDER] = Default::default();
//         let myslots = self.slots.borrow();
//         for i in 0..BTREE_ORDER {
//             slots[i] = myslots[i].clone();
//         }
//         Self {
//             n: self.n.clone(),
//             items: self.items.clone(),
//             slots: PRefCell::new(slots),
//         }
//     }
// }

// pub struct VBTree<V> {
//     root: PRefCell<PmemObj<Node<V>>>,
// }

// impl<V: Default> Default for NodeItem<V> {
//     fn default() -> Self {
//         Self {
//             key: 0,
//             val: Default::default(),
//         }
//     }
// }

// impl<V: Default> NodeItem<V> {
//     #[inline]
//     fn empty(&mut self) {
//         self.key = 0;
//         self.val = Default::default();
//     }
// }

// impl<V: Default + Copy> Node<V> {
//     #[inline]
//     fn new() -> Self {
//         Self {
//             n: PRefCell::new(0),
//             items: Default::default(),
//             slots: Default::default(),
//         }
//     }

//     #[inline]
//     fn insert_node(&self, p: usize, item: NodeItem<V>, left: Rc<Node<V>>, right: Rc<Node<V>>) {
//         let mut self_items = self.items.borrow_mut();
//         let mut self_slots = self.slots.borrow_mut();
//         if self_items[p].key != 0 {
//             /* move all existing data */
//             self_items[p..].rotate_right(1);
//             self_slots[p..].rotate_right(1);
//         }
//         self_slots[p] = Some(left);
//         self_slots[p + 1] = Some(right);
//         self.insert_item_at(p, item);
//     }

//     #[inline]
//     fn insert_item_at(&self, pos: usize, item: NodeItem<V>) {
//         let mut self_items = self.items.borrow_mut();
//         let mut n = self.n.borrow_mut();
//         self_items[pos] = item;
//         *n += 1;
//     }

//     #[inline]
//     fn clear_node(&self) {
//         let mut slots = self.slots.borrow_mut();
//         for n in &mut *slots {
//             *n = None;
//         }
//     }

//     #[inline]
//     fn split(&self, m: &mut NodeItem<V>) -> Self {
//         let mut self_items = self.items.borrow_mut();
//         let mut self_slots = self.slots.borrow_mut();
//         let mut self_n = self.n.borrow_mut();

//         let right = Node::<V>::new();
//         let c = BTREE_ORDER / 2;
//         *m = self_items[c - 1]; /* select median item */
//         self_items[c - 1].empty();

//         /* move everything right side of median to the new node */
//         let mut right_items = right.items.borrow_mut();
//         let mut right_slots = right.slots.borrow_mut();
//         let mut right_n = right.n.borrow_mut();
//         for i in c..BTREE_ORDER {
//             if i != BTREE_ORDER - 1 {
//                 right_items[*right_n] = self_items[i];
//                 *right_n += 1;
//                 self_items[i].empty();
//             }
//             right_slots[i - c] = self_slots[i].clone();
//             self_slots[i] = None;
//         }
//         *self_n = c - 1;
//         right.clone()
//     }

//     #[inline]
//     fn insert_item(&self, p: usize, item: NodeItem<V>) {
//         let mut self_items = self.items.borrow_mut();
//         if self_items[p].key != 0 {
//             self_items[p..].rotate_right(1);
//         }
//         self.insert_item_at(p, item);
//     }

//     #[inline]
//     fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
//         let self_items = self.items.borrow();
//         let self_slots = self.slots.borrow();
//         let self_n = self.n.borrow();

//         for i in 0..*self_n + 1 {
//             if let Some(p) = &self_slots[i] {
//                 if p.foreach(f) {
//                     return true;
//                 }
//             }

//             if i != *self_n && self_items[i].key != 0 {
//                 if f(&self_items[i].key, &self_items[i].val) {
//                     return true;
//                 }
//             }
//         }
//         false
//     }

//     #[inline]
//     fn lookup(&self, key: u64) -> Option<&V> {
//         let self_items = self.items.borrow();
//         let self_slots = self.slots.borrow();
//         let self_n = self.n.borrow();
//         for i in 0..*self_n {
//             if i < *self_n {
//                 if self_items[i].key == key {
//                     return Some(&self_items[i].val);
//                 } else {
//                     if self_items[i].key > key {
//                         return if let Some(slot) = &self_slots[i] {
//                             slot.lookup(key)
//                         } else {
//                             None
//                         };
//                     }
//                 }
//             } else {
//                 return if let Some(slot) = &self_slots[i] {
//                     slot.lookup(key)
//                 } else {
//                     None
//                 };
//             }
//         }
//         None
//     }

//     #[inline]
//     fn remove(&self, p: usize) {
//         let mut self_items = self.items.borrow_mut();
//         let mut self_n = self.n.borrow_mut();
//         self_items[p].empty();
//         if *self_n != 1 && p != BTREE_ORDER - 2 {
//             self_items[p..].rotate_left(1);
//         }
//         *self_n -= 1;
//     }

//     #[inline]
//     fn rotate_right(&self, node: Rc<Node<V>>, parent: Rc<Node<V>>, p: usize) {
//         let mut self_items = self.items.borrow_mut();
//         let mut self_slots = self.slots.borrow_mut();
//         let mut self_n = self.n.borrow_mut();
//         let mut parent_items = parent.items.borrow_mut();
//         let mut node_slots = node.slots.borrow_mut();
//         let node_n = node.n.borrow();

//         let sep = parent_items[p];
//         node.insert_item(*node_n, sep);

//         parent_items[p] = self_items[0];

//         node_slots[*node_n] = self_slots[0].clone();
//         self_slots[0] = None;

//         *self_n -= 1;
//         self_slots.rotate_left(1);
//         self_items.rotate_left(1);
//     }

//     #[inline]
//     fn rotate_left(&self, node: Rc<Node<V>>, parent: Rc<Node<V>>, p: usize) {
//         let self_items = self.items.borrow();
//         let mut self_slots = self.slots.borrow_mut();
//         let mut self_n = self.n.borrow_mut();
//         let mut parent_items = parent.items.borrow_mut();
//         let mut node_slots = node.slots.borrow_mut();

//         let sep = parent_items[p - 1];
//         node.insert_item(0, sep);

//         parent_items[p - 1] = self_items[*self_n - 1];

//         node_slots.rotate_right(1);
//         node_slots[0] = self_slots[*self_n].clone();
//         self_slots[*self_n] = None;

//         *self_n -= 1;
//     }
// }

// impl<V: Default + Copy + Debug> VBTree<V>
// where
//     NodeItem<V>: Clone,
// {
//     #[inline]
//     fn find_dest_node_in<'a>(&'a self, n: Rc<Node<V>>, key: u64, p: &mut usize) -> Rc<Node<V>> {
//         let n_items = n.items.borrow();
//         let n_slots = n.slots.borrow();
//         let n_n = n.n.borrow();

//         for i in 0..BTREE_ORDER - 1 {
//             *p = i;

//             /*
//              * The key either fits somewhere in the middle or at the
//              * right edge of the node.
//              */
//             if *n_n == i || n_items[i].key > key {
//                 let slot = n_slots[i].clone();
//                 if let Some(slot) = slot {
//                     return self.find_dest_node(slot, Some(n.clone()), key, p);
//                 } else {
//                     return n.clone();
//                 }
//             }
//         }

//         let slot = n_slots[BTREE_ORDER - 1].clone();
//         if let Some(slot) = slot {
//             self.find_dest_node(slot, Some(n.clone()), key, p)
//         } else {
//             n.clone()
//         }
//     }

//     #[inline]
//     fn find_dest_node<'a>(
//         &'a self,
//         n: Rc<Node<V>>,
//         parent: Option<Rc<Node<V>>>,
//         key: u64,
//         p: &mut usize,
//     ) -> Rc<Node<V>> {
//         let n_n = n.n.borrow();

//         if *n_n == BTREE_ORDER - 1 {
//             /* node is full, perform a split */
//             let mut m = NodeItem::default();
//             let right = n.split(&mut m);

//             let right = Rc::new(right);
//             if let Some(parent) = parent {
//                 if key > m.key {
//                     /* select node to continue search */
//                     parent.insert_node(*p, m, n.clone(), right.clone());
//                     self.find_dest_node_in(right, key, p)
//                 } else {
//                     self.find_dest_node_in(n.clone(), key, p)
//                 }
//             } else {
//                 /* replacing root node, the tree grows in height */

//                 let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
//                 let mut slots: [PmemObj<Node<V>>; BTREE_ORDER] = Default::default();
//                 items[0] = m;
//                 slots[0] = Some(n.clone());
//                 slots[1] = Some(right);
//                 let up = Rc::new(Node {
//                     n: PRefCell::new(1),
//                     items: PRefCell::new(items),
//                     slots: PRefCell::new(slots),
//                 });
//                 let mut root = self.root.borrow_mut();
//                 *root = Some(up.clone());

//                 self.find_dest_node_in(up, key, p)
//             }
//         } else {
//             self.find_dest_node_in(n.clone(), key, p)
//         }
//     }

//     #[inline]
//     fn insert_empty(&self, item: NodeItem<V>) {
//         let mut root = self.root.borrow_mut();
//         let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
//         items[0] = item;
//         *root = Some(Rc::new(Node {
//             n: PRefCell::new(1),
//             items: PRefCell::new(items),
//             slots: Default::default(),
//         }));
//     }

//     #[inline]
//     fn get_leftmost_leaf<'a>(&self, n: Rc<Node<V>>, p: &mut Rc<Node<V>>) -> Rc<Node<V>> {
//         let n_slots = n.slots.borrow();
//         if let Some(slot) = &n_slots[0] {
//             *p = n.clone();
//             self.get_leftmost_leaf(slot.clone(), p)
//         } else {
//             n.clone()
//         }
//     }

//     #[inline]
//     fn remove_from(&self, node: Rc<Node<V>>, p: usize) {
//         let mut node_items = node.items.borrow_mut();
//         let node_slots = node.slots.borrow();
//         if node_slots[0].is_none() {
//             /* leaf */
//             node.remove(p);
//         } else {
//             let mut lp = node.clone();
//             let lm = if let Some(slot) = &node_slots[p + 1] {
//                 self.get_leftmost_leaf(slot.clone(), &mut lp)
//             } else {
//                 node.clone()
//             };
//             let lm_items = lm.items.borrow();
//             let lm_n = lm.n.borrow();

//             node_items[p] = lm_items[0];
//             self.remove_from(lm.clone(), 0);

//             if *lm_n < BTREE_MIN {
//                 self.rebalance(
//                     lm.clone(),
//                     lp.clone(),
//                     if &*lp as *const _ == &*node as *const _ {
//                         p + 1
//                     } else {
//                         0
//                     },
//                 );
//             }
//         }
//     }

//     fn remove_item(
//         &self,
//         node: Rc<Node<V>>,
//         parent: Option<Rc<Node<V>>>,
//         key: u64,
//         p: usize,
//     ) -> Option<V> {
//         let mut ret = None;
//         let node_items = node.items.borrow();
//         let node_slots = node.slots.borrow();
//         let node_n = node.n.borrow();
//         for i in 0..*node_n + 1 {
//             if i != *node_n && node_items[i].key == key {
//                 ret = Some(node_items[i].val);
//                 self.remove_from(node.clone(), i);
//                 break;
//             } else if let Some(slot) = &node_slots[i] {
//                 if i == *node_n || node_items[i].key > key {
//                     ret = self.remove_item(slot.clone(), Some(node.clone()), key, i);
//                     break;
//                 }
//             }
//         }

//         if let Some(parent) = parent {
//             if *node_n < BTREE_MIN {
//                 self.rebalance(node.clone(), parent, p);
//             }
//         }

//         ret
//     }

//     fn rebalance(&self, node: Rc<Node<V>>, parent: Rc<Node<V>>, p: usize) {
//         let parent_slots = parent.slots.borrow();
//         let parent_n = parent.n.borrow();

//         let rsb = if p >= *parent_n {
//             &None
//         } else {
//             &parent_slots[p + 1]
//         };
//         let lsb = if p == 0 { &None } else { &parent_slots[p - 1] };
//         if let Some(rsb) = rsb {
//             let rsb_n = rsb.n.borrow();
//             if *rsb_n > BTREE_MIN {
//                 rsb.rotate_right(node, parent.clone(), p);
//                 return;
//             }
//         }
//         if let Some(lsb) = lsb {
//             let lsb_n = lsb.n.borrow();
//             if *lsb_n > BTREE_MIN {
//                 lsb.rotate_left(node, parent.clone(), p);
//                 return;
//             }
//         }
//         if let Some(rsb) = rsb {
//             self.merge(rsb.clone(), node, parent.clone(), p)
//         } else if let Some(lsb) = lsb {
//             self.merge(node, lsb.clone(), parent.clone(), p - 1)
//         }
//     }

//     #[inline]
//     fn merge(&self, rn: Rc<Node<V>>, node: Rc<Node<V>>, parent: Rc<Node<V>>, p: usize) {
//         let mut node_items = node.items.borrow_mut();
//         let mut node_slots = node.slots.borrow_mut();
//         let mut node_n = node.n.borrow_mut();
//         let mut parent_items = parent.items.borrow_mut();
//         let mut parent_slots = parent.slots.borrow_mut();
//         let mut parent_n = parent.n.borrow_mut();
//         let rn_items = rn.items.borrow();
//         let rn_slots = rn.slots.borrow();
//         let rn_n = rn.n.borrow();

//         let sep = parent_items[p];
//         node_items[*node_n] = sep;
//         *node_n += 1;

//         node_items[*node_n..*node_n + *rn_n].copy_from_slice(&rn_items[0..*rn_n]);
//         for i in 0..*rn_n + 1 {
//             node_slots[*node_n + i] = rn_slots[i].clone();
//         }
//         *node_n += *rn_n;
//         *parent_n -= 1;
//         parent_items[p..].rotate_left(1);
//         parent_slots[p + 1..].rotate_left(1);
//         parent_items.last_mut().unwrap().empty();
//         *parent_slots.last_mut().unwrap() = None;

//         if *parent_n == 0 {
//             let mut root = self.root.borrow_mut();
//             *root = Some(node.clone());
//         }
//     }
// }

// impl<V: Default + Debug> Map<u64, V> for VBTree<V>
// where
//     NodeItem<V>: Clone,
//     V: UnwindSafe + Copy,
// {
//     fn clear(&self) {
//         if let Some(root) = &mut *self.root.borrow_mut() {
//             *root = Rc::new(Node::new());
//         }
//     }

//     fn insert(&self, key: u64, val: V) {
//         let item = NodeItem::<V> { key, val };
//         if self.is_empty() {
//             self.insert_empty(item);
//         } else {
//             let mut p = 0;
//             let root = self.root.borrow();
//             if let Some(root) = &*root {
//                 let dest = self.find_dest_node(root.clone(), None, key, &mut p);
//                 dest.insert_item(p, item);
//             }
//         }
//     }

//     fn remove(&self, key: u64) {
//         if let Some(root) = &*self.root.borrow() {
//             self.remove_item(root.clone(), None, key, 0);
//         }
//     }

//     fn is_empty(&self) -> bool {
//         if let Some(root) = &*self.root.borrow() {
//             let root_n = root.n.borrow();
//             *root_n == 0
//         } else {
//             true
//         }
//     }

//     fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
//         if let Some(root) = &*self.root.borrow() {
//             root.foreach(f)
//         } else {
//             false
//         }
//     }

//     fn lookup(&self, key: u64) -> Option<&V> {
//         if let Some(root) = &*self.root.borrow() {
//             root.lookup(key)
//         } else {
//             None
//         }
//     }
// }

// impl<V> Default for VBTree<V> {
//     fn default() -> Self {
//         VBTree {
//             root: PRefCell::new(None),
//         }
//     }
// }
