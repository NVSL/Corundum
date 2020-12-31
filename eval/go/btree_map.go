package main

import (
	"flag"
	"os"
	"bufio"
	"fmt"
	"math/rand"
	"strings"

	"github.com/vmware/go-pmem-transaction/pmem"
	"github.com/vmware/go-pmem-transaction/transaction"
)

const BTREE_ORDER int = 8
const BTREE_MIN int = ((BTREE_ORDER / 2) - 1)

type item struct {
	key int
	value int
}

type node_t struct {
	n     int
	items [BTREE_ORDER-1]item
	slots [BTREE_ORDER]*node_t
}

type data struct {
	root  *node_t
	magic int
}

const (
	// A magic number used to identify if the root object initialization
	// completed successfully.
	magic = 0x1B2E8BFF7BFBD154
)

func initialize(ptr *data) {
	{
		ptr.root = nil
		ptr.magic = magic
	}
}

/*
 * set_empty_item -- (internal) sets nil to the item
 */
func set_empty_item(item *item) {
	item.key = 0
	item.value = 0
}

/*
 * btree_map_clear_node -- (internal) removes all elements from the node_t
 */
func btree_map_clear_node(node *node_t) {
	if node == nil {
		return
	}
	for i := 0; i < node.n; i++ {
		btree_map_clear_node(node.slots[i])
	}
}

/*
 * btree_map_clear -- removes all elements from the ptr
 */
func btree_map_clear(ptr *data) int{
	txn("undo") {
		btree_map_clear_node(ptr.root)
		ptr.root = nil
	}
	return 0
}

/*
 * btree_map_insert_item_at -- (internal) inserts an item at position
 */
func btree_map_insert_item_at(node *node_t, pos int, item item) {
	node.items[pos] = item
	node.n += 1
}

/*
 * btree_map_insert_empty -- (internal) inserts an item into an empty node_t
 */
func btree_map_insert_empty(ptr *data, item item) {
	ptr.root = pnew(node_t)
	ptr.root.n = 0

	btree_map_insert_item_at(ptr.root, 0, item)
}

/*
 * btree_map_insert_node -- (internal) inserts and makes space for new node_t
 */
func btree_map_insert_node(node *node_t, p int, item item, left *node_t, right *node_t) {
	if node.items[p].key != 0 { /* move all existing data */
		copy(node.items[p+1:], node.items[p:])
		copy(node.slots[p+1:], node.slots[p:])
	}
	node.slots[p] = left
	node.slots[p + 1] = right
	btree_map_insert_item_at(node, p, item)
}

/*
 * btree_map_create_split_node -- (internal) splits a node_t into two
 */
func btree_map_create_split_node(node *node_t, m *item) *node_t {
	right := pnew(node_t)
	right.n = 0

	c := (BTREE_ORDER / 2)
	*m = node.items[c - 1]; /* select median item */
	set_empty_item(&node.items[c - 1])

	/* move everything right side of median to the new node_t */
	for i := c; i < BTREE_ORDER; i++ {
		if i != BTREE_ORDER - 1 {
			right.items[right.n] = node.items[i]
			right.n++
			set_empty_item(&node.items[i])
		}
		right.slots[i - c] = node.slots[i]
		node.slots[i] = nil
	}
	node.n = c - 1

	return right
}

/*
 * btree_map_find_dest_node -- (internal) finds a place to insert the new key at
 */
func btree_map_find_dest_node(ptr *data, n *node_t, 
	parent *node_t, key int, p *int) *node_t {
	if n.n == BTREE_ORDER - 1 { /* node_t is full, perform a split */
		var m item
		right := btree_map_create_split_node(n, &m)

		if parent != nil {
			btree_map_insert_node(parent, *p, m, n, right)
			if key > m.key { /* select node_t to continue search */
				n = right
			}
		} else { /* replacing root node_t, the tree grows in height */
			up := pnew(node_t)
			up.n = 1
			up.items[0] = m
			up.slots[0] = n
			up.slots[1] = right

			ptr.root = up
			n = up
		}
	}

	var i int
	for i = 0; i < BTREE_ORDER - 1; i++ {
		*p = i

		/*
		* The key either fits somewhere in the middle or at the
		* right edge of the node.
 		*/
		if n.n == i || n.items[i].key > key {
			if n.slots[i] == nil {
				return n
			} else {
				return btree_map_find_dest_node(ptr, n.slots[i], n, key, p)
			}
		}
	}

	/*
	 * The key is bigger than the last node_t element, go one level deeper
	 * in the rightmost child.
 	 */
	return btree_map_find_dest_node(ptr, n.slots[i], n, key, p)
}

/*
 * btree_map_insert_item -- (internal) inserts and makes space for new item
 */
func btree_map_insert_item(node *node_t, p int, item item) {
	if node.items[p].key != 0 {
		copy(node.items[p+1:], node.items[p:])
	}
	btree_map_insert_item_at(node, p, item)
}

/*
 * btree_map_is_empty -- checks whether the tree ptr is empty
 */
func btree_map_is_empty(ptr *data) bool {
	return ptr.root == nil || ptr.root.n == 0
}

/*
 * btree_map_insert -- inserts a new key-value pair into the ptr
 */
func btree_map_insert(ptr *data, key int, value int) bool {
	item := item {key, value}
	txn("undo") {
		if btree_map_is_empty(ptr) {
			btree_map_insert_empty(ptr, item)
		} else {
			var p int /* position at the dest node_t to insert */
			var parent *node_t = nil
			var dest *node_t = btree_map_find_dest_node(ptr, ptr.root, parent, key, &p)

			btree_map_insert_item(dest, p, item)
		}
	}
	return true
}

/*
 * btree_map_rotate_right -- (internal) takes one element from right sibling
 */
func btree_map_rotate_right(rsb *node_t, node *node_t, parent *node_t, p int) {
	/* move the separator from parent to the deficient node_t */
	sep := parent.items[p]
	btree_map_insert_item(node, node.n, sep)

	/* the first element of the right sibling is the new separator */
	parent.items[p] = rsb.items[0]

	/* the nodes are not necessarily leafs, so copy also the slot */
	node.slots[node.n] = rsb.slots[0]

	rsb.n -= 1 /* it loses one element, but still > min */

	/* move all existing elements back by one array slot */
	copy(rsb.items[:], rsb.items[1:])
	copy(rsb.slots[:], rsb.slots[1:])
}

/*
 * btree_map_rotate_left -- (internal) takes one element from left sibling
 */
func btree_map_rotate_left(lsb *node_t, node *node_t, parent *node_t, p int) {
	/* move the separator from parent to the deficient node_t */
	sep := parent.items[p - 1]
	btree_map_insert_item(node, 0, sep)

	/* the last element of the left sibling is the new separator */
	parent.items[p - 1] = lsb.items[lsb.n - 1]

	/* rotate the node_t children */
	copy(node.slots[1:], node.slots[:])

	/* the nodes are not necessarily leafs, so copy also the slot */
	node.slots[0] = lsb.slots[lsb.n]

	lsb.n -= 1 /* it loses one element, but still > min */
}

/*
 * btree_map_merge -- (internal) merges node_t and right sibling
 */
func btree_map_merge(ptr *data, rn *node_t, node *node_t, parent *node_t, p int) {
	sep := parent.items[p]

	/* add separator to the deficient node_t */
	node.items[node.n] = sep
	node.n++

	/* copy right sibling data to node_t */
	copy(node.items[node.n:], rn.items[:])
	copy(node.slots[node.n:], rn.slots[:])

	node.n += rn.n
	parent.n -= 1

	/* move everything to the right of the separator by one array slot */
	copy(parent.items[p:], parent.items[p+1:])

	copy(parent.slots[p+1:], parent.slots[p+2:])

	/* if the parent is empty then the tree shrinks in height */
	if parent.n == 0 && parent == ptr.root {
		ptr.root = node
	}
}

/*
 * btree_map_rebalance -- (internal) performs tree rebalance
 */
func btree_map_rebalance(ptr *data, node *node_t, parent *node_t, p int) {
	var rsb *node_t = nil
	if p < parent.n {
		rsb = parent.slots[p + 1]
	}
	var lsb *node_t = nil
	if p != 0 {
		lsb = parent.slots[p - 1]
	}

	if rsb != nil && rsb.n > BTREE_MIN {
		btree_map_rotate_right(rsb, node, parent, p)
	} else if lsb != nil && lsb.n > BTREE_MIN {
		btree_map_rotate_left(lsb, node, parent, p)
	} else if rsb == nil { /* always merge with rightmost node_t */
		btree_map_merge(ptr, node, lsb, parent, p - 1)
	} else {
		btree_map_merge(ptr, rsb, node, parent, p)
	}
}

/*
 * btree_map_get_leftmost_leaf -- (internal) searches for the successor
 */
func btree_map_get_leftmost_leaf(ptr *data, n *node_t, p **node_t) *node_t {
	if n.slots[0] == nil {
		return n
	}
	*p = n
	return btree_map_get_leftmost_leaf(ptr, n.slots[0], p)
}

/*
 * btree_map_remove_from_node -- (internal) removes element from node_t
 */
func btree_map_remove_from_node(ptr *data, node *node_t, parent *node_t, p int) {
	if node.slots[0] == nil { /* leaf */
		if node.n == 1 || p == BTREE_ORDER - 2 {
			set_empty_item(&node.items[p])
		} else if node.n != 1 {
			copy(node.items[p:], node.items[p+1:])
		}
		node.n -= 1
		return
	}

	/* can't delete from non-leaf nodes, remove successor */
	var rchild *node_t = node.slots[p + 1]
	var lp *node_t = node
	var lm *node_t = btree_map_get_leftmost_leaf(ptr, rchild, &lp)

	node.items[p] = lm.items[0]

	btree_map_remove_from_node(ptr, lm, lp, 0)

	if lm.n < BTREE_MIN { /* right child can be deficient now */
		if lp == node {
			btree_map_rebalance(ptr, lm, lp, p+1)
		} else {
			btree_map_rebalance(ptr, lm, lp, 0)
		}
	}
}

// #define node_contains_item(_n, _i, _k)\
// ((_i) != _n.n && _n.items[_i].key == (_k))

// #define node_child_can_contain_item(_n, _i, _k)\
// ((_i) == _n.n || _n.items[_i].key > (_k)) &&\
// _n.slots[_i] != nil

func node_contains_item(n *node_t, i int, k int) bool {
	return i != n.n && n.items[i].key == k
}

func node_child_can_contain_item(n *node_t, i int, k int) bool {
	return (i != n.n || n.items[i].key > k) && n.slots[i] != nil
}

/*
 * btree_map_remove_item -- (internal) removes item from node_t
 */
func btree_map_remove_item(ptr *data, node *node_t, parent *node_t, key int, p int) int {
	ret := 0
	for i := 0; i <= node.n; i++ {
		if node_contains_item(node, i, key) {
			ret = node.items[i].value
			btree_map_remove_from_node(ptr, node, parent, i)
			break
		} else if node_child_can_contain_item(node, i, key) {
			ret = btree_map_remove_item(ptr, node.slots[i],
				node, key, i)
			break
		}
	}

	/* check for deficient nodes walking up */
	if parent != nil && node.n < BTREE_MIN {
		btree_map_rebalance(ptr, node, parent, p)
	}

	return ret
}

/*
 * btree_map_remove -- removes key-value pair from the ptr
 */
func btree_map_remove(ptr *data, key int) int {
	ret := 0
	txn("undo") {
		ret = btree_map_remove_item(ptr, ptr.root, nil, key, 0)
	}
	return ret
}

/*
 * btree_map_get_in_node -- (internal) searches for a value in the node_t
 */
func btree_map_get_in_node(node *node_t, key int) int {
	for i := 0; i <= node.n; i++ {
		if node_contains_item(node, i, key) {
			return node.items[i].value
		} else if node_child_can_contain_item(node, i, key) {
			return btree_map_get_in_node(node.slots[i], key)
		}
	}

	return -1
}

/*
 * btree_map_get -- searches for a value of the key
 */
func btree_map_get(ptr *data, key int) int {
	if ptr.root == nil {
		return 0
	}
	return btree_map_get_in_node(ptr.root, key)
}

/*
 * btree_map_lookup_in_node -- (internal) searches for key if exists
 */
func btree_map_lookup_in_node(node *node_t, key int) bool {
	for i := 0; i <= node.n; i++ {
		if node_contains_item(node, i, key) {
			return true
		} else if node_child_can_contain_item(node, i, key) {
			return btree_map_lookup_in_node(node.slots[i], key)
		}
	}
	return false
}

/*
 * btree_map_lookup -- searches if key exists
 */
func btree_map_lookup(ptr *data, key int) bool {
	if ptr.root == nil {
		return false
	}
	return btree_map_lookup_in_node(ptr.root, key)
}

/*
 * btree_map_foreach_node -- (internal) recursively traverses tree
 */
func btree_map_foreach_node(p *node_t, cb func(int, int) bool) bool {
	if p == nil {
		return false
	}

	for i := 0; i <= p.n; i++ {
		if btree_map_foreach_node(p.slots[i], cb) {
			return true
		}

		if i != p.n && p.items[i].key != 0 {
			if cb(p.items[i].key, p.items[i].value) {
				return true
			}
		}
	}
	return false
}

/*
 * btree_map_foreach -- initiates recursive traversal
 */
func btree_map_foreach(ptr *data, cb func(int, int) bool) bool {
	return btree_map_foreach_node(ptr.root, cb)
}

/*
 * ctree_map_check -- check if given persistent object is a tree ptr
 */
func btree_map_check(ptr *data) bool {
	return ptr == nil // || !TOID_VALID(ptr)
}

/*
 * btree_map_remove_free -- removes and frees an object from the tree
 */
func btree_map_remove_free(ptr *data, key int) bool {
	txn("undo") {
		btree_map_remove(ptr, key)
	}
	return true
}

/*
 * str_insert -- hs_insert wrapper which works on strings
 */
func str_insert(ptr *data, str string) {
	var key int
	if _, err := fmt.Sscanf(str, "%d", &key); err == nil {
		btree_map_insert(ptr, key, 0)
	} else {
		fmt.Println("insert: invalid syntax")
	}
}

/*
 * str_remove -- hs_remove wrapper which works on strings
 */
func str_remove(ptr *data, str string) {
	var key int
	if _, err := fmt.Sscanf(str, "%d", &key); err == nil {
		if btree_map_lookup(ptr, key) {
			btree_map_remove(ptr, key)
		} else {
			fmt.Println("no such value")
		}
	} else {
		fmt.Println("remove: invalid syntax")
	}
}

/*
 * str_check -- hs_check wrapper which works on strings
 */
func str_check(ptr *data, str string) {
	var key int
	if _, err := fmt.Sscanf(str, "%d", &key); err == nil {
		fmt.Println(btree_map_lookup(ptr, key))
	} else {
		fmt.Println("check: invalid syntax")
	}
}

/*
 * str_insert_random -- inserts specified (as string) number of random numbers
 */
func str_insert_random(ptr *data, str string) {
	var val int
	if _, err := fmt.Sscanf(str, "%d", &val); err == nil {
		for i := 0; i < val; i++ {
			r := rand.Int()
			if !btree_map_insert(ptr, r, 0) {
				break
			}
		}
	} else {
		fmt.Println("random insert: invalid syntax")
	}
}

func help() {
	fmt.Println("h - help")
	fmt.Println("i $value - insert $value")
	fmt.Println("r $value - remove $value")
	fmt.Println("c $value - check $value, returns 0/1")
	fmt.Println("n $value - insert $value random values")
	fmt.Println("p - print all values")
	fmt.Println("d - print debug info")
	fmt.Println("q - quit")
}

func unknown_command(str string) {
	fmt.Println("unknown command '",str,"', use 'h' for help")
}

func hashmap_print(key int, val int) bool {
	fmt.Print(key, " ")
	return false
}

func print_all(ptr *data) {
	btree_map_foreach(ptr, hashmap_print)
	fmt.Println()
}

func main() {
	args := os.Args

	if len(args) < 2 {
		fmt.Println("usage:", args[0], "filename")
		return
	}

	var ptr *data
	flag.Parse()
	firstInit := pmem.Init(args[1])
	if firstInit {
		// first time run of the application
		ptr = (*data)(pmem.New("root", ptr))
		initialize(ptr)
	} else {
		// not a first time initialization
		ptr = (*data)(pmem.Get("root", ptr))

		// even though this is not a first time initialization, we should still
		// check if the named object exists and data initialization completed
		// succesfully. The magic element within the named object helps check
		// for successful data initialization.

		if ptr == nil {
			ptr = (*data)(pmem.New("root", ptr))
		}

		if ptr.magic != magic {
			initialize(ptr)
		}
	}
	reader := bufio.NewReader(os.Stdin)
	for {
		fmt.Print("$ ")
		buf, _ := reader.ReadString('\n')
		// convert CRLF to LF
		buf = strings.Replace(buf, "\n", "", -1)

		if buf[0] == 0 || buf[0] == '\n' {
			continue
		}

		switch (buf[0]) {
			case 'i': str_insert(ptr, buf[1:])
			case 'r': str_remove(ptr, buf[1:])
			case 'c': str_check(ptr, buf[1:])
			case 'n': str_insert_random(ptr, buf[1:])
			case 'p': print_all(ptr)
			case 'q': return
			case 'h': help()
			default: unknown_command(buf)
		}
	}
}

