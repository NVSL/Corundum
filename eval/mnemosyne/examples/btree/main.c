#include "pvar.h"
#include <stdio.h>
#include <stdlib.h>
#include <pmalloc.h>
#include <assert.h>
#include <string.h>

void initialize() {
    int found = 0;
    PTx {
        if (PGET(root) == NULL) {
            node_t *node = (node_t*)pmalloc(sizeof(node_t));
            node->key = -1;
            node->value[0] = 0;
            node->slots[0] = node->slots[1] = NULL;
            PSET(root, node);
        } else {
            found = 1;
        }
    }
    if (!found) {
        fprintf(stderr, "Created the root object.\n");
    } else {
        fprintf(stderr, "Found the root object.\n");
    }
}

void print_node(struct node_t *n, int *elem) {
    if (n) {
        (*elem)++;
        print_node(n->slots[0], elem);
        fprintf(stderr, "%s ", n->value);
        print_node(n->slots[1], elem);
    }
}

void print() {
    node_t *btree = NULL;
    PTx { btree = PGET(root); }
    assert(btree);

    fprintf(stderr, "Contents of existing btree: ");
    int elem_count = 0;
    print_node(btree, &elem_count);
    fprintf(stderr, "\nelem_count = %d\n", elem_count);
}

void btree_insert(int64_t key, const char *value) {
    node_t *btree = NULL;
    PTx { btree = PGET(root); }
    assert(btree);
    int len = strlen(value);
    len = (len < 32) ? len : 32;
    PTx {
        node_t *node = (node_t *)pmalloc(sizeof(node_t));
        node->key = key;
        if (value) {
            memcpy(node->value, value, len);
        }
        node->slots[0] = node->slots[1] = NULL;

        node_t **dst = &btree;
        while (*dst) {
            dst = &(*dst)->slots[key > (*dst)->key];
        }
        *dst = node;
    }
}

char *btree_find(int64_t key) {
    node_t *btree = NULL;
    PTx { btree = PGET(root); }
    assert(btree);

    node_t *n = btree;
    while (n) {
        if (n->key == key) return n->value;
        n = n->slots[key > n->key];
    }

    return NULL;
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
		printf("usage: %s [p|i|f|s|r] [key] [value] \n", argv[0]);
		return 1;
	}

    initialize();

    const char op = argv[1][0];
	int64_t key,k,len;
	const char *value;

    switch (op) {
		case 'p':
			print();
		break;
		case 'i':
			key = atoll(argv[2]);
			value = argv[3];
			btree_insert(key, value);
		break;
		case 'f':
			key = atoll(argv[2]);
			if ((value = btree_find(key)) != NULL)
				printf("%s\n", value);
			else
				printf("not found\n");
		break;
        case 's':
        	len = atoll(argv[2]);
                for(k=0; k<len; k++) {
                    btree_insert(k, "test");
                }
        break;
        case 'r':
                len = atoll(argv[2]);
                for(long k=0; k<len; k++) {
                    value=btree_find(k);
                }
		printf("last value = %s\n", value);
        break;
		default:
			printf("invalid operation\n");
		break;
	}

    return 0;
}
