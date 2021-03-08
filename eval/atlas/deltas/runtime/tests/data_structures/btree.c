#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Atlas includes
#include "atlas_alloc.h"
#include "atlas_api.h"

typedef struct node_t {
    int64_t key;
    char value[32];
    struct node_t *slots[2];
} node_t;

typedef struct btree_t {
    node_t *root;
    pthread_mutex_t *root_lock;
} btree_t;

btree_t *B;

int ready = 0;
int done = 0;

pthread_mutex_t ready_lock;
pthread_mutex_t done_lock;

// ID of Atlas persistent region
uint32_t btree_rgn_id;

void initialize() {
    void *rgn_root = NVM_GetRegionRoot(btree_rgn_id);
    if (rgn_root) {
        B = (btree_t *)rgn_root;
        B->root_lock = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
        pthread_mutex_init(B->root_lock, NULL);

        fprintf(stderr, "Found btree at %p\n", (void *)B);
    } else {
        node_t *node = (node_t *)nvm_alloc(sizeof(node_t), btree_rgn_id);
        node->key = -1;
        node->value[0] = 0;
        node->slots[0] = node->slots[1] = NULL;
        B = (btree_t *)nvm_alloc(sizeof(btree_t), btree_rgn_id);
        fprintf(stderr, "Created B at %p\n", (void *)B);

        B->root_lock = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
        pthread_mutex_init(B->root_lock, NULL);

        NVM_BEGIN_DURABLE();

        B->root = node;

        // Set the root of the Atlas persistent region
        NVM_SetRegionRoot(btree_rgn_id, B);

        NVM_END_DURABLE();
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
    assert(B);
    assert(B->root);

    fprintf(stderr, "Contents of existing btree: ");
    int elem_count = 0;
    print_node(B->root, &elem_count);
    fprintf(stderr, "\nelem_count = %d\n", elem_count);
}

void btree_insert(int64_t key, const char *value) {
    assert(B);
    assert(B->root);

    node_t *node = (node_t *)nvm_alloc(sizeof(node_t), btree_rgn_id);
    node->key = key;
    strcpy(node->value, value);
    node->slots[0] = node->slots[1] = NULL;

    pthread_mutex_lock(B->root_lock);
    node_t **dst = &B->root;
    while (*dst) {
        dst = &(*dst)->slots[key > (*dst)->key];
    }
    *dst = node;
    pthread_mutex_unlock(B->root_lock);
}

char *btree_find(int64_t key) {
    assert(B);
    assert(B->root);

    node_t *n = B->root;
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

    // Initialize Atlas
    NVM_Initialize();
    // Create an Atlas persistent region
    btree_rgn_id = NVM_FindOrCreateRegion("btree", O_RDWR, NULL);
    // This contains the Atlas restart code to find any reusable data
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

    // Close the Atlas persistent region
    NVM_CloseRegion(btree_rgn_id);
    // Atlas bookkeeping
    NVM_Finalize();

    return 0;
}
