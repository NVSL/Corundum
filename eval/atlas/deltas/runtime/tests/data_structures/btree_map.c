// SPDX-License-Identifier: BSD-3-Clause
/* Copyright 2015-2019, Intel Corporation */

/*
 * btree_map.c -- textbook implementation of btree /w preemptive splitting
 */

#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <inttypes.h>
#include <string.h>
#include <unistd.h>

#include "atlas_alloc.h"
#include "atlas_api.h"

#define BTREE_ORDER 8 /* can't be odd */
#define BTREE_MIN ((BTREE_ORDER / 2) - 1) /* min number of keys per node */

struct tree_map_node_item {
	uint64_t key;
	void *value;
};

typedef struct node_t {
	int n; /* number of occupied slots */
	struct tree_map_node_item items[BTREE_ORDER - 1];
	struct node_t *slots[BTREE_ORDER];
} node_t;

typedef struct btree_map {
	node_t *root;
    pthread_mutex_t *root_lock;
} btree_map;

btree_map *map;

// ID of Atlas persistent region
uint32_t btree_rgn_id;

void initialize() {
    void *rgn_root = NVM_GetRegionRoot(btree_rgn_id);
    if (rgn_root) {
        map = (btree_map *)rgn_root;
        map->root_lock = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
        pthread_mutex_init(map->root_lock, NULL);

        fprintf(stderr, "Found btree at %p\n", (void *)map);
    } else {
        node_t *node = (node_t *)nvm_alloc(sizeof(node_t), btree_rgn_id);
        node->n = 0;
        map = (btree_map *)nvm_alloc(sizeof(btree_map), btree_rgn_id);
        fprintf(stderr, "Created map at %p\n", (void *)map);

        map->root_lock = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
        pthread_mutex_init(map->root_lock, NULL);

        NVM_BEGIN_DURABLE();

        map->root = node;

        // Set the root of the Atlas persistent region
        NVM_SetRegionRoot(btree_rgn_id, map);

        NVM_END_DURABLE();
    }
}

/*
 * set_empty_item -- (internal) sets null to the item
 */
static void
set_empty_item(struct tree_map_node_item *item)
{
	item->key = 0;
	item->value = NULL;
}

/*
 * btree_map_clear_node -- (internal) removes all elements from the node
 */
static void
btree_map_clear_node(node_t *node)
{
	if (node == NULL)
		return;
	for (int i = 0; i < node->n; ++i) {
		btree_map_clear_node(node->slots[i]);
	}

	nvm_free(node);
}

/*
 * btree_map_clear -- removes all elements from the map
 */
int
btree_map_clear()
{
    pthread_mutex_lock(map->root_lock);
    btree_map_clear_node(map->root);
    map->root = NULL;
    pthread_mutex_unlock(map->root_lock);
	return 0;
}

/*
 * btree_map_insert_item_at -- (internal) inserts an item at position
 */
static void
btree_map_insert_item_at(node_t *node, int pos,
	struct tree_map_node_item item)
{
	node->items[pos] = item;
	node->n += 1;
}

/*
 * btree_map_insert_empty -- (internal) inserts an item into an empty node
 */
static void
btree_map_insert_empty(struct btree_map *map,
	struct tree_map_node_item item)
{
	map->root = (node_t*)nvm_alloc(sizeof(node_t), btree_rgn_id);
    map->root->n = 0;

	btree_map_insert_item_at(map->root, 0, item);
}

/*
 * btree_map_insert_node -- (internal) inserts and makes space for new node
 */
static void
btree_map_insert_node(node_t *node, int p,
	struct tree_map_node_item item,
	node_t *left, node_t *right)
{
	if (node->items[p].key != 0) { /* move all existing data */
		memmove(&node->items[p + 1], &node->items[p],
		sizeof(struct tree_map_node_item) * ((BTREE_ORDER - 2 - p)));

		memmove(&node->slots[p + 1], &node->slots[p],
		sizeof(node_t*) * ((BTREE_ORDER - 1 - p)));
	}
	node->slots[p] = left;
	node->slots[p + 1] = right;
	btree_map_insert_item_at(node, p, item);
}

/*
 * btree_map_create_split_node -- (internal) splits a node into two
 */
static node_t*
btree_map_create_split_node(node_t *node,
	struct tree_map_node_item *m)
{
	node_t *right = (node_t*)nvm_alloc(sizeof(node_t), btree_rgn_id);
    right->n = 0;

	int c = (BTREE_ORDER / 2);
	*m = node->items[c - 1]; /* select median item */
	set_empty_item(&node->items[c - 1]);

	/* move everything right side of median to the new node */
	for (int i = c; i < BTREE_ORDER; ++i) {
		if (i != BTREE_ORDER - 1) {
			right->items[right->n++] =
				node->items[i];
			set_empty_item(&node->items[i]);
		}
		right->slots[i - c] = node->slots[i];
		node->slots[i] = NULL;
	}
	node->n = c - 1;

	return right;
}

/*
 * btree_map_find_dest_node -- (internal) finds a place to insert the new key at
 */
static node_t*
btree_map_find_dest_node(struct btree_map *map,
	node_t *n, node_t *parent,
	uint64_t key, int *p)
{
	if (n->n == BTREE_ORDER - 1) { /* node is full, perform a split */
		struct tree_map_node_item m;
		node_t *right =
			btree_map_create_split_node(n, &m);

		if (parent != NULL) {
			btree_map_insert_node(parent, *p, m, n, right);
			if (key > m.key) /* select node to continue search */
				n = right;
		} else { /* replacing root node, the tree grows in height */
			node_t *up = (node_t*)nvm_alloc(sizeof(node_t), btree_rgn_id);
			up->n = 1;
			up->items[0] = m;
			up->slots[0] = n;
			up->slots[1] = right;
            
			map->root = up;
			n = up;
		}
	}

	int i;
	for (i = 0; i < BTREE_ORDER - 1; ++i) {
		*p = i;

		/*
		 * The key either fits somewhere in the middle or at the
		 * right edge of the node.
		 */
		if (n->n == i || n->items[i].key > key) {
			return n->slots[i] == NULL ? n :
				btree_map_find_dest_node(map,
					n->slots[i], n, key, p);
		}
	}

	/*
	 * The key is bigger than the last node element, go one level deeper
	 * in the rightmost child.
	 */
	return btree_map_find_dest_node(map, n->slots[i], n, key, p);
}

/*
 * btree_map_insert_item -- (internal) inserts and makes space for new item
 */
static void
btree_map_insert_item(node_t *node, int p,
	struct tree_map_node_item item)
{
	if (node->items[p].key != 0) {
		memmove(&node->items[p + 1], &node->items[p],
		sizeof(struct tree_map_node_item) * ((BTREE_ORDER - 2 - p)));
	}
	btree_map_insert_item_at(node, p, item);
}

/*
 * btree_map_is_empty -- checks whether the tree map is empty
 */
int
btree_map_is_empty()
{
	return map->root == NULL || map->root->n == 0;
}

/*
 * btree_map_insert -- inserts a new key-value pair into the map
 */
int
btree_map_insert(
	uint64_t key, void *value)
{
	struct tree_map_node_item item = {key, value};
    pthread_mutex_lock(map->root_lock);
    if (btree_map_is_empty()) {
        btree_map_insert_empty(map, item);
    } else {
        int p; /* position at the dest node to insert */
        node_t *parent = NULL;
        node_t *dest =
            btree_map_find_dest_node(map, map->root,
                parent, key, &p);

        btree_map_insert_item(dest, p, item);
    }
    pthread_mutex_unlock(map->root_lock);

	return 0;
}

/*
 * btree_map_rotate_right -- (internal) takes one element from right sibling
 */
static void
btree_map_rotate_right(node_t *rsb,
	node_t *node,
	node_t *parent, int p)
{
	/* move the separator from parent to the deficient node */
	struct tree_map_node_item sep = parent->items[p];
	btree_map_insert_item(node, node->n, sep);

	/* the first element of the right sibling is the new separator */
	parent->items[p] = rsb->items[0];

	/* the nodes are not necessarily leafs, so copy also the slot */
	node->slots[node->n] = rsb->slots[0];

	rsb->n -= 1; /* it loses one element, but still > min */

	/* move all existing elements back by one array slot */
	memmove(rsb->items, rsb->items + 1,
		sizeof(struct tree_map_node_item) * (rsb->n));
	memmove(rsb->slots, rsb->slots + 1,
		sizeof(node_t*) * (rsb->n + 1));
}

/*
 * btree_map_rotate_left -- (internal) takes one element from left sibling
 */
static void
btree_map_rotate_left(node_t *lsb,
	node_t *node,
	node_t *parent, int p)
{
	/* move the separator from parent to the deficient node */
	struct tree_map_node_item sep = parent->items[p - 1];
	btree_map_insert_item(node, 0, sep);

	/* the last element of the left sibling is the new separator */
	parent->items[p - 1] = lsb->items[lsb->n - 1];

	/* rotate the node children */
	memmove(node->slots + 1, node->slots,
		sizeof(node_t*) * (node->n));

	/* the nodes are not necessarily leafs, so copy also the slot */
	node->slots[0] = lsb->slots[lsb->n];

	lsb->n -= 1; /* it loses one element, but still > min */
}

/*
 * btree_map_merge -- (internal) merges node and right sibling
 */
static void
btree_map_merge(struct btree_map *map, node_t *rn,
	node_t *node,
	node_t *parent, int p)
{
	struct tree_map_node_item sep = parent->items[p];

	/* add separator to the deficient node */
	node->items[node->n++] = sep;

	/* copy right sibling data to node */
	memcpy(&node->items[node->n], rn->items,
	sizeof(struct tree_map_node_item) * rn->n);
	memcpy(&node->slots[node->n], rn->slots,
	sizeof(node_t*) * (rn->n + 1));

	node->n += rn->n;

	nvm_free(rn); /* right node is now empty */

	parent->n -= 1;

	/* move everything to the right of the separator by one array slot */
	memmove(parent->items + p, parent->items + p + 1,
	sizeof(struct tree_map_node_item) * (parent->n - p));

	memmove(parent->slots + p + 1, parent->slots + p + 2,
	sizeof(node_t*) * (parent->n - p + 1));

	/* if the parent is empty then the tree shrinks in height */
	if (parent->n == 0 && parent == map->root) {
		nvm_free(map->root);
		map->root = node;
	}
}

/*
 * btree_map_rebalance -- (internal) performs tree rebalance
 */
static void
btree_map_rebalance(struct btree_map *map, node_t *node,
	node_t *parent, int p)
{
	node_t *rsb = p >= parent->n ?
		NULL : parent->slots[p + 1];
	node_t *lsb = p == 0 ?
		NULL : parent->slots[p - 1];

	if (rsb != NULL && rsb->n > BTREE_MIN)
		btree_map_rotate_right(rsb, node, parent, p);
	else if (lsb != NULL && lsb->n > BTREE_MIN)
		btree_map_rotate_left(lsb, node, parent, p);
	else if (rsb == NULL) /* always merge with rightmost node */
		btree_map_merge(map, node, lsb, parent, p - 1);
	else
		btree_map_merge(map, rsb, node, parent, p);
}

/*
 * btree_map_get_leftmost_leaf -- (internal) searches for the successor
 */
static node_t*
btree_map_get_leftmost_leaf(struct btree_map *map,
	node_t *n, node_t **p)
{
	if (n->slots[0] == NULL)
		return n;

	*p = n;

	return btree_map_get_leftmost_leaf(map, n->slots[0], p);
}

/*
 * btree_map_remove_from_node -- (internal) removes element from node
 */
static void
btree_map_remove_from_node(struct btree_map *map,
	node_t *node,
	node_t *parent, int p)
{
	if (node->slots[0] == NULL) { /* leaf */
		if (node->n == 1 || p == BTREE_ORDER - 2) {
			set_empty_item(&node->items[p]);
		} else if (node->n != 1) {
			memmove(&node->items[p],
				&node->items[p + 1],
				sizeof(struct tree_map_node_item) *
				(node->n - p));
		}

		node->n -= 1;
		return;
	}

	/* can't delete from non-leaf nodes, remove successor */
	node_t *rchild = node->slots[p + 1];
	node_t *lp = node;
	node_t *lm =
		btree_map_get_leftmost_leaf(map, rchild, &lp);

	node->items[p] = lm->items[0];

	btree_map_remove_from_node(map, lm, lp, 0);

	if (lm->n < BTREE_MIN) /* right child can be deficient now */
		btree_map_rebalance(map, lm, lp,
			lp == node ? p + 1 : 0);
}

#define NODE_CONTAINS_ITEM(_n, _i, _k)\
((_i) != _n->n && _n->items[_i].key == (_k))

#define NODE_CHILD_CAN_CONTAIN_ITEM(_n, _i, _k)\
((_i) == _n->n || _n->items[_i].key > (_k)) &&\
_n->slots[_i] != NULL

/*
 * btree_map_remove_item -- (internal) removes item from node
 */
static void*
btree_map_remove_item(struct btree_map *map,
	node_t *node, node_t *parent,
	uint64_t key, int p)
{
	void *ret = NULL;
	for (int i = 0; i <= node->n; ++i) {
		if (NODE_CONTAINS_ITEM(node, i, key)) {
			ret = node->items[i].value;
			btree_map_remove_from_node(map, node, parent, i);
			break;
		} else if (NODE_CHILD_CAN_CONTAIN_ITEM(node, i, key)) {
			ret = btree_map_remove_item(map, node->slots[i],
				node, key, i);
			break;
		}
	}

	/* check for deficient nodes walking up */
	if (parent != NULL && node->n < BTREE_MIN)
		btree_map_rebalance(map, node, parent, p);

	return ret;
}

/*
 * btree_map_remove -- removes key-value pair from the map
 */
void*
btree_map_remove(uint64_t key)
{
	void *ret = NULL;
	
    pthread_mutex_lock(map->root_lock);
    ret = btree_map_remove_item(map, map->root, NULL, key, 0);
    pthread_mutex_unlock(map->root_lock);

	return ret;
}

/*
 * btree_map_get_in_node -- (internal) searches for a value in the node
 */
static void*
btree_map_get_in_node(node_t *node, uint64_t key)
{
	for (int i = 0; i <= node->n; ++i) {
		if (NODE_CONTAINS_ITEM(node, i, key))
			return node->items[i].value;
		else if (NODE_CHILD_CAN_CONTAIN_ITEM(node, i, key))
			return btree_map_get_in_node(node->slots[i], key);
	}

	return NULL;
}

/*
 * btree_map_get -- searches for a value of the key
 */
void*
btree_map_get(uint64_t key)
{
	if (map->root == NULL)
		return NULL;
	return btree_map_get_in_node(map->root, key);
}

/*
 * btree_map_lookup_in_node -- (internal) searches for key if exists
 */
static int
btree_map_lookup_in_node(node_t *node, uint64_t key)
{
	for (int i = 0; i <= node->n; ++i) {
		if (NODE_CONTAINS_ITEM(node, i, key))
			return 1;
		else if (NODE_CHILD_CAN_CONTAIN_ITEM(node, i, key))
			return btree_map_lookup_in_node(
					node->slots[i], key);
	}

	return 0;
}

/*
 * btree_map_lookup -- searches if key exists
 */
int
btree_map_lookup(uint64_t key)
{
	if (map->root == NULL)
		return 0;
	return btree_map_lookup_in_node(map->root, key);
}

/*
 * btree_map_foreach_node -- (internal) recursively traverses tree
 */
static int
btree_map_foreach_node(const node_t *p,
	int (*cb)(uint64_t key, void*, void *arg), void *arg)
{
	if (p == NULL)
		return 0;

	for (int i = 0; i <= p->n; ++i) {
		if (btree_map_foreach_node(p->slots[i], cb, arg) != 0)
			return 1;

		if (i != p->n && p->items[i].key != 0) {
			if (cb(p->items[i].key, p->items[i].value,
					arg) != 0)
				return 1;
		}
	}
	return 0;
}

/*
 * btree_map_foreach -- initiates recursive traversal
 */
int
btree_map_foreach(
	int (*cb)(uint64_t key, void *value, void *arg), void *arg)
{
	return btree_map_foreach_node(map->root, cb, arg);
}

/*
 * ctree_map_check -- check if given persistent object is a tree map
 */
int
btree_map_check()
{
	return map == NULL; // || !TOID_VALID(map);
}

/*
 * btree_map_remove_free -- removes and frees an object from the tree
 */
int
btree_map_remove_free(
		uint64_t key)
{
    pthread_mutex_lock(map->root_lock);
    void *val = btree_map_remove(key);
    if(val) nvm_free(val);
    pthread_mutex_unlock(map->root_lock);

	return 0;
}

/*
 * str_insert -- hs_insert wrapper which works on strings
 */
static void
str_insert(const char *str)
{
	uint64_t key;
	if (sscanf(str, "%lu", &key) > 0)
		btree_map_insert(key, NULL);
	else
		fprintf(stderr, "insert: invalid syntax\n");
}

/*
 * str_remove -- hs_remove wrapper which works on strings
 */
static void
str_remove(const char *str)
{
	uint64_t key;
	if (sscanf(str, "%lu", &key) > 0) {
		int l = btree_map_lookup(key);
		if (l)
			btree_map_remove(key);
		else
			fprintf(stderr, "no such value\n");
	} else
		fprintf(stderr,	"remove: invalid syntax\n");
}

/*
 * str_check -- hs_check wrapper which works on strings
 */
static void
str_check(const char *str)
{
	uint64_t key;
	if (sscanf(str, "%lu", &key) > 0) {
		int r = btree_map_lookup(key);
		printf("%d\n", r);
	} else {
		fprintf(stderr, "check: invalid syntax\n");
	}
}

/*
 * str_insert_random -- inserts specified (as string) number of random numbers
 */
static void
str_insert_random(const char *str)
{
	uint64_t val;
	if (sscanf(str, "%lu", &val) > 0)
		for (uint64_t i = 0; i < val; ) {
			uint64_t r = ((uint64_t)rand()) << 32 | rand();
			int ret = btree_map_insert(r, NULL);
			if (ret < 0)
				break;
			if (ret == 0)
				i += 1;
		}
	else
		fprintf(stderr, "random insert: invalid syntax\n");
}

static void
help(void)
{
	printf("h - help\n");
	printf("i $value - insert $value\n");
	printf("r $value - remove $value\n");
	printf("c $value - check $value, returns 0/1\n");
	printf("n $value - insert $value random values\n");
	printf("p - print all values\n");
	printf("d - print debug info\n");
	printf("q - quit\n");
}

static void
unknown_command(const char *str)
{
	fprintf(stderr, "unknown command '%c', use 'h' for help\n", str[0]);
}

static int
hashmap_print(uint64_t key, void *value, void *arg)
{
	printf("%lu ", key);
	return 0;
}

static void
print_all(void)
{
	btree_map_foreach(hashmap_print, NULL);
	printf("\n");
}

#define INPUT_BUF_LEN 1000
int
main()
{
	char buf[INPUT_BUF_LEN];

    // Initialize Atlas
    NVM_Initialize();
    // Create an Atlas persistent region
    btree_rgn_id = NVM_FindOrCreateRegion("btree_map", O_RDWR, NULL);
    // This contains the Atlas restart code to find any reusable data
    initialize();

	if (isatty(fileno(stdout)))
		printf("Type 'h' for help\n$ ");

	while (fgets(buf, sizeof(buf), stdin)) {
		if (buf[0] == 0 || buf[0] == '\n')
			continue;

		switch (buf[0]) {
			case 'i':
				str_insert(buf + 1);
				break;
			case 'r':
				str_remove(buf + 1);
				break;
			case 'c':
				str_check(buf + 1);
				break;
			case 'n':
				str_insert_random(buf + 1);
				break;
			case 'p':
				print_all();
				break;
			case 'q':
				fclose(stdin);
				break;
			case 'h':
				help();
				break;
			default:
				unknown_command(buf);
				break;
		}

		if (isatty(fileno(stdout)))
			printf("$ ");
	}

    // Close the Atlas persistent region
    NVM_CloseRegion(btree_rgn_id);
    // Atlas bookkeeping
    NVM_Finalize();

	return 0;
}
