// //! btree.rs -- textbook implementation of btree /w preemptive splitting
// //! equivalent to [btree example] from PMDK.
// //!
// //! [btree example]: https://github.com/pmem/pmdk/blob/master/src/examples/libpmemobj/tree_map/btree_map.c

// use crate::map::*;
// use corundum::alloc::heap::*;
// use corundum::boxed::Pbox;
// use corundum::cell::*;
// use corundum::clone::PClone;
// use corundum::ptr::*;
// use corundum::stm::Journal;
// use corundum::*;
// use std::fmt::Debug;
// use std::panic::UnwindSafe;

// type PmemObj<T> = Pbox<PRefCell<T, P>, P>;

// const BTREE_ORDER: usize = 8;
// const BTREE_MIN: usize = (BTREE_ORDER / 2) - 1;

// #[derive(Copy, Debug)]
// pub struct NodeItem<V: PSafe + PSafe> {
//     key: u64,
//     val: V,
// }

// // use std::cell::UnsafeCell;
// // use std::panic::RefUnwindSafe;
// // use std::marker::PhantomData;
// // use std::ops::DerefMut;
// // use std::ops::Deref;
// // use std::fmt::Debug;
// // use std::panic::UnwindSafe;
// // struct PRefCell<T: ?Sized, A: MemPool> (PhantomData<A>,UnsafeCell<T>);
// // impl<T, A: MemPool> PRefCell<T, A> {
// //     pub fn new(n: T, _j: &Journal<A>) -> Self {
// //         Self(PhantomData,UnsafeCell::new(n))
// //     }
// // }
// // impl<T: ?Sized, A: MemPool> PSafe for PRefCell<T, A> {}
// // unsafe impl<T: ?Sized, A: MemPool> TxInSafe for PRefCell<T, A> {}
// // impl<T: ?Sized, A: MemPool> UnwindSafe for PRefCell<T, A> {}
// // impl<T: ?Sized, A: MemPool> RefUnwindSafe for PRefCell<T, A> {}
// // impl<T: ?Sized, A: MemPool> PRefCell<T, A> {
// //     pub fn borrow(&self) -> &T {
// //         unsafe {&*self.1.get()}
// //     }
// //     pub fn borrow_mut(&self, _j: &Journal<A>) -> LogRefMut<T> {
// //         unsafe {LogRefMut{data: &mut *self.1.get()}}
// //     }
// // }
// // impl<T: ?Sized, A: MemPool> Deref for PRefCell<T, A> {
// //     type Target = T;
// //     fn deref(&self) -> &T { unsafe {&*self.1.get()} }
// // }
// // impl<T: PClone<A>, A: MemPool> PClone<A> for PRefCell<T, A> {
// //     fn pclone(&self, j: &Journal<A>) -> Self {
// //         Self(PhantomData,UnsafeCell::new(self.borrow().pclone(j)))
// //     }
// // }
// // impl<T: ?Sized + Default, A: MemPool> Default for PRefCell<T, A> {
// //     fn default() -> Self { Self(PhantomData,Default::default()) }
// // }
// // impl<T: ?Sized + Debug, A: MemPool> Debug for PRefCell<T, A> {
// // fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> { todo!() }
// // }
// // struct LogRefMut<'a, T: ?Sized> {
// //     data: &'a mut T
// // }
// // impl<T: ?Sized> Deref for LogRefMut<'_, T> {
// //     type Target = T;
// //     fn deref(&self) -> &T { self.data }
// // }
// // impl<T: ?Sized> DerefMut for LogRefMut<'_, T> {
// //     fn deref_mut(&mut self) -> &mut T { self.data }
// // }

// impl<V: PSafe + PSafe + Copy> Clone for NodeItem<V> {
//     fn clone(&self) -> Self {
//         Self {
//             key: self.key,
//             val: self.val,
//         }
//     }
// }

// #[derive(Debug)]
// pub struct Node<V: PSafe> {
//     n: usize, // Number of occupied slots
//     items: [NodeItem<V>; BTREE_ORDER - 1],
//     slots: [Option<PmemObj<Node<V>>>; BTREE_ORDER],
// }

// impl<V: PSafe + Copy> PClone<P> for Node<V> {
//     #[inline]
//     fn pclone(&self, j: &Journal<P>) -> Self {
//         let mut slots: [Option<PmemObj<Node<V>>>; BTREE_ORDER] = Default::default();
//         let myslots = &(*self).slots;
//         for i in 0..BTREE_ORDER {
//             slots[i] = myslots[i].pclone(j);
//         }
//         Self {
//             n: self.n,
//             items: self.items,
//             slots,
//         }
//     }
// }

// pub struct UBTree<V: PSafe> {
//     root: PRefCell<Option<PmemObj<Node<V>>>, P>,
// }

// impl<V: PSafe + Default> Default for NodeItem<V> {
//     fn default() -> Self {
//         Self {
//             key: 0,
//             val: Default::default(),
//         }
//     }
// }

// impl<V: PSafe + Default> NodeItem<V> {
//     #[inline]
//     fn empty(&mut self) {
//         self.key = 0;
//         self.val = Default::default();
//     }
// }

// impl<V: PSafe + Default + Copy> Node<V> {
//     #[inline]
//     fn def() -> Self {
//         Self {
//             n: 0,
//             items: Default::default(),
//             slots: Default::default(),
//         }
//     }

//     #[inline]
//     fn new(j: &Journal<P>) -> Pbox<PRefCell<Self, P>, P> {
//         Pbox::new(PRefCell::new(Self::def(), j), j)
//     }

//     #[inline]
//     fn insert_node<'a>(
//         &'a mut self,
//         p: &mut usize,
//         item: NodeItem<V>,
//         left: PmemObj<Node<V>>,
//         right: PmemObj<Node<V>>,
//     ) -> Ptr<PmemObj<Node<V>>, P> {
//         let p = *p;
//         if self.items[p].key != 0 {
//             /* move all existing data */
//             self.items[p..].rotate_right(1);
//             self.slots[p..].rotate_right(1);
//         }
//         self.slots[p] = Some(left);
//         self.slots[p + 1] = Some(right);
//         self.insert_item_at(p, item);
//         if let Some(slot) = &self.slots[p + 1] {
//             Ptr::from(slot)
//         } else {
//             Ptr::dangling()
//         }
//     }

//     #[inline]
//     fn insert_item_at(&mut self, pos: usize, item: NodeItem<V>) {
//         self.items[pos] = item;
//         self.n += 1;
//     }

//     #[inline]
//     fn clear_node(&mut self) {
//         for n in &mut self.slots {
//             *n = None;
//         }
//     }

//     #[inline]
//     fn split<'a>(&'a mut self, m: &mut NodeItem<V>, j: &'a Journal<P>) -> PmemObj<Self> {
//         let mut right = Node::<V>::def();
//         let c = BTREE_ORDER / 2;
//         *m = self.items[c - 1]; /* select median item */
//         self.items[c - 1].empty();

//         /* move everything right side of median to the new node */
//         for i in c..BTREE_ORDER {
//             if i != BTREE_ORDER - 1 {
//                 right.items[right.n] = self.items[i];
//                 right.n += 1;
//                 self.items[i].empty();
//             }
//             right.slots[i - c] = self.slots[i].pclone(j);
//             self.slots[i] = None;
//         }
//         self.n = c - 1;
//         Pbox::new(PRefCell::new(right, j), j)
//     }

//     #[inline]
//     fn insert_item(&mut self, p: usize, item: NodeItem<V>) {
//         if self.items[p].key != 0 {
//             self.items[p..].rotate_right(1);
//         }
//         self.insert_item_at(p, item);
//     }

//     #[inline]
//     fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
//         for i in 0..self.n + 1 {
//             if let Some(p) = &self.slots[i] {
//                 if p.foreach(f) {
//                     return true;
//                 }
//             }

//             if i != self.n && self.items[i].key != 0 {
//                 if f(&self.items[i].key, &self.items[i].val) {
//                     return true;
//                 }
//             }
//         }
//         false
//     }

//     #[inline]
//     fn lookup(&self, key: u64) -> Option<&V> {
//         for i in 0..self.n {
//             if i < self.n {
//                 if self.items[i].key == key {
//                     return Some(&self.items[i].val);
//                 } else {
//                     if self.items[i].key > key {
//                         return if let Some(slot) = &self.slots[i] {
//                             slot.lookup(key)
//                         } else {
//                             None
//                         };
//                     }
//                 }
//             } else {
//                 return if let Some(slot) = &self.slots[i] {
//                     slot.lookup(key)
//                 } else {
//                     None
//                 };
//             }
//         }
//         None
//     }

//     #[inline]
//     fn remove(&mut self, p: usize) {
//         self.items[p].empty();
//         if self.n != 1 && p != BTREE_ORDER - 2 {
//             self.items[p..].rotate_left(1);
//         }
//         self.n -= 1;
//     }

//     #[inline]
//     fn rotate_right(
//         &mut self,
//         node: Ptr<PmemObj<Node<V>>, P>,
//         parent: Ptr<PmemObj<Node<V>>, P>,
//         p: usize,
//         j: &Journal<P>,
//     ) {
//         let sep = parent.items[p];
//         let mut node_mut = node.borrow_mut(j);
//         let mut parent = parent.borrow_mut(j);

//         node_mut.insert_item(node.n, sep);

//         parent.items[p] = self.items[0];

//         node_mut.slots[node.n] = self.slots[0].pclone(j);
//         self.slots[0] = None;

//         self.n -= 1;
//         self.slots.rotate_left(1);
//         self.items.rotate_left(1);
//     }

//     #[inline]
//     fn rotate_left(
//         &mut self,
//         node: Ptr<PmemObj<Node<V>>, P>,
//         parent: Ptr<PmemObj<Node<V>>, P>,
//         p: usize,
//         j: &Journal<P>,
//     ) {
//         let sep = parent.items[p - 1];
//         let mut node_mut = node.borrow_mut(j);
//         let mut parent = parent.borrow_mut(j);

//         node_mut.insert_item(0, sep);

//         parent.items[p - 1] = self.items[self.n - 1];

//         node_mut.slots.rotate_right(1);
//         node_mut.slots[0] = self.slots[self.n].pclone(j);
//         self.slots[self.n] = None;

//         self.n -= 1;
//     }
// }

// impl<V: PSafe + Default + Copy + Debug> UBTree<V> {
//     #[inline]
//     fn set_root<'a>(
//         &'a self,
//         up: PmemObj<Node<V>>,
//         j: &'a Journal<P>,
//     ) -> &'a Option<PmemObj<Node<V>>> {
//         let mut root = self.root.borrow_mut(j);
//         *root = Some(up);
//         &self.root
//     }

//     #[inline]
//     fn find_dest_node_in<'a>(
//         &'a self,
//         n: Ptr<PmemObj<Node<V>>, P>,
//         key: u64,
//         p: &mut usize,
//         j: &'a Journal<P>,
//     ) -> Ptr<PmemObj<Node<V>>, P> {
//         for i in 0..BTREE_ORDER - 1 {
//             *p = i;

//             /*
//              * The key either fits somewhere in the middle or at the
//              * right edge of the node.
//              */
//             if n.n == i || n.items[i].key > key {
//                 if let Some(slot) = &n.slots[i] {
//                     return self.find_dest_node(Ptr::from(slot), Some(n), key, p, j);
//                 } else {
//                     return n;
//                 }
//             }
//         }

//         if let Some(slot) = &n.slots[BTREE_ORDER - 1] {
//             self.find_dest_node(Ptr::from(slot), Some(n), key, p, j)
//         } else {
//             n
//         }
//     }

//     #[inline]
//     fn find_dest_node<'a>(
//         &'a self,
//         n: Ptr<PmemObj<Node<V>>, P>,
//         parent: Option<Ptr<PmemObj<Node<V>>, P>>,
//         key: u64,
//         p: &mut usize,
//         j: &'a Journal<P>,
//     ) -> Ptr<PmemObj<Node<V>>, P> {
//         if n.n == BTREE_ORDER - 1 {
//             /* node is full, perform a split */
//             let mut m = NodeItem::default();
//             let mut n_mut = n.borrow_mut(j);
//             let right = n_mut.split(&mut m, j);

//             if let Some(parent) = parent {
//                 if key > m.key {
//                     /* select node to continue search */
//                     let mut parent = parent.borrow_mut(j);
//                     let right = parent.insert_node(p, m, (&*n).pclone(j), right);
//                     self.find_dest_node_in(right, key, p, j)
//                 } else {
//                     self.find_dest_node_in(n, key, p, j)
//                 }
//             } else {
//                 /* replacing root node, the tree grows in height */
//                 let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
//                 let mut slots: [Option<PmemObj<Node<V>>>; BTREE_ORDER] = Default::default();
//                 items[0] = m;
//                 slots[0] = Some((&*n).pclone(j));
//                 slots[1] = Some(right);
//                 let up = Pbox::new(
//                     PRefCell::new(
//                         Node {
//                             n: 1,
//                             items: items,
//                             slots: slots,
//                         },
//                         j,
//                     ),
//                     j,
//                 );
//                 if let Some(up) = &self.set_root(up, j) {
//                     self.find_dest_node_in(Ptr::from(up), key, p, j)
//                 } else {
//                     n
//                 }
//             }
//         } else {
//             self.find_dest_node_in(n, key, p, j)
//         }
//     }

//     #[inline]
//     fn insert_empty(&self, item: NodeItem<V>, j: &Journal<P>) {
//         let mut root = self.root.borrow_mut(j);
//         let mut items: [NodeItem<V>; BTREE_ORDER - 1] = Default::default();
//         items[0] = item;
//         *root = Some(Pbox::new(
//             PRefCell::new(
//                 Node {
//                     n: 1,
//                     items,
//                     slots: Default::default(),
//                 },
//                 j,
//             ),
//             j,
//         ));
//     }

//     #[inline]
//     fn get_leftmost_leaf<'a>(
//         &self,
//         n: Ptr<PmemObj<Node<V>>, P>,
//         p: &mut Ptr<PmemObj<Node<V>>, P>,
//         j: &'a Journal<P>,
//     ) -> Ptr<PmemObj<Node<V>>, P> {
//         if let Some(slot) = &n.slots[0] {
//             *p = n;
//             self.get_leftmost_leaf(Ptr::from(slot), p, j)
//         } else {
//             n
//         }
//     }

//     #[inline]
//     fn remove_from(&self, node: Ptr<PmemObj<Node<V>>, P>, p: usize, j: &Journal<P>) {
//         let mut n = node.borrow_mut(j);
//         if node.slots[0].is_none() {
//             /* leaf */
//             n.remove(p);
//         } else {
//             let mut lp = node;
//             let lm = if let Some(slot) = &node.slots[p + 1] {
//                 self.get_leftmost_leaf(Ptr::from(slot), &mut lp, j)
//             } else {
//                 node
//             };
//             n.items[p] = lm.items[0];
//             self.remove_from(lm, 0, j);

//             if lm.n < BTREE_MIN {
//                 self.rebalance(lm, lp, if lp == node { p + 1 } else { 0 }, j);
//             }
//         }
//     }

//     fn remove_item(
//         &self,
//         node: Ptr<PmemObj<Node<V>>, P>,
//         parent: Option<Ptr<PmemObj<Node<V>>, P>>,
//         key: u64,
//         p: usize,
//         j: &Journal<P>,
//     ) -> Option<V> {
//         let mut ret = None;
//         for i in 0..node.n + 1 {
//             if i != node.n && node.items[i].key == key {
//                 ret = Some(node.items[i].val);
//                 self.remove_from(node, i, j);
//                 break;
//             } else if let Some(slot) = &node.slots[i] {
//                 if i == node.n || node.items[i].key > key {
//                     ret = self.remove_item(Ptr::from(slot), Some(node), key, i, j);
//                     break;
//                 }
//             }
//         }

//         if let Some(parent) = parent {
//             if node.n < BTREE_MIN {
//                 self.rebalance(node, parent, p, j);
//             }
//         }

//         ret
//     }

//     fn rebalance(
//         &self,
//         node: Ptr<PmemObj<Node<V>>, P>,
//         parent: Ptr<PmemObj<Node<V>>, P>,
//         p: usize,
//         j: &Journal<P>,
//     ) {
//         let mut rsb = if p >= parent.n {
//             Ptr::dangling()
//         } else {
//             if let Some(slot) = &parent.slots[p + 1] {
//                 Ptr::from(slot)
//             } else {
//                 Ptr::dangling()
//             }
//         };
//         let mut lsb = if p == 0 {
//             Ptr::dangling()
//         } else {
//             if let Some(slot) = &parent.slots[p - 1] {
//                 Ptr::from(slot)
//             } else {
//                 Ptr::dangling()
//             }
//         };
//         if let Some(rsb) = rsb.as_option() {
//             if rsb.n > BTREE_MIN {
//                 rsb.borrow_mut(j).rotate_right(node, parent, p, j);
//                 return;
//             }
//         }
//         if let Some(lsb) = lsb.as_option() {
//             if lsb.n > BTREE_MIN {
//                 lsb.borrow_mut(j).rotate_left(node, parent, p, j);
//                 return;
//             }
//         }
//         if let Some(rsb) = rsb.as_option() {
//             self.merge(*rsb, node, parent, p, j)
//         } else if let Some(lsb) = lsb.as_option() {
//             self.merge(node, *lsb, parent, p - 1, j)
//         }
//     }

//     #[inline]
//     fn merge(
//         &self,
//         rn: Ptr<PmemObj<Node<V>>, P>,
//         node: Ptr<PmemObj<Node<V>>, P>,
//         parent: Ptr<PmemObj<Node<V>>, P>,
//         p: usize,
//         j: &Journal<P>,
//     ) {
//         let sep = parent.items[p];
//         let mut node_mut = node.borrow_mut(j);
//         node_mut.items[node.n] = sep;
//         node_mut.n += 1;

//         node_mut.items[node.n..node.n + rn.n].copy_from_slice(&rn.items[0..rn.n]);
//         for i in 0..rn.n + 1 {
//             node_mut.slots[node.n + i] = rn.slots[i].pclone(j);
//         }
//         node_mut.n += rn.n;

//         let mut parent = parent.borrow_mut(j);
//         parent.n -= 1;
//         parent.items[p..].rotate_left(1);
//         parent.slots[p + 1..].rotate_left(1);
//         parent.items.last_mut().unwrap().empty();
//         *parent.slots.last_mut().unwrap() = None;

//         if parent.n == 0 {
//             self.set_root((&*node).pclone(j), j);
//         }
//     }
// }

// impl<V: PSafe + Default + Debug> Map<u64, V> for UBTree<V>
// where
//     NodeItem<V>: Clone,
//     V: std::panic::RefUnwindSafe + TxInSafe + UnwindSafe + Copy,
// {
//     fn clear(&self) {
//         P::transaction(|j| {
//             if let Some(root) = &mut *self.root.borrow_mut(j) {
//                 *root = Node::new(j);
//             }
//         })
//         .unwrap();
//     }

//     fn insert(&self, key: u64, val: V) {
//         P::transaction(|j| {
//             let item = NodeItem::<V> { key, val };
//             if self.is_empty() {
//                 self.insert_empty(item, j);
//             } else {
//                 let mut p = 0;
//                 if let Some(root) = &*self.root {
//                     let dest = self.find_dest_node(Ptr::from(root), None, key, &mut p, j);
//                     let mut dest = dest.borrow_mut(j);
//                     dest.insert_item(p, item);
//                 }
//             }
//         })
//         .unwrap();
//     }

//     fn remove(&self, key: u64) {
//         P::transaction(|j| {
//             if let Some(root) = &mut *self.root.borrow_mut(j) {
//                 self.remove_item(Ptr::from(root), None, key, 0, j);
//             }
//         })
//         .unwrap();
//     }

//     fn is_empty(&self) -> bool {
//         if let Some(root) = &*self.root {
//             root.n == 0
//         } else {
//             true
//         }
//     }

//     fn foreach<F: Copy + Fn(&u64, &V) -> bool>(&self, f: F) -> bool {
//         if let Some(root) = &*self.root {
//             root.foreach(f)
//         } else {
//             false
//         }
//     }

//     fn lookup(&self, key: u64) -> Option<&V> {
//         if let Some(root) = &*self.root {
//             root.lookup(key)
//         } else {
//             None
//         }
//     }
// }

// impl<V: PSafe> RootObj<P> for UBTree<V> {
//     fn init(j: &Journal<P>) -> Self {
//         UBTree {
//             root: PRefCell::new(None, j),
//         }
//     }
// }
