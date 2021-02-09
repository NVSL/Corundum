#include "ex_common.h"
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <libpmemobj.h>

POBJ_LAYOUT_BEGIN(alloc);
POBJ_LAYOUT_TOID(alloc, uint8_t);
POBJ_LAYOUT_END(alloc);

PMEMobjpool *pop;
int len = 512;
int cnt = 128;
int thr = 1;

void *worker(void *vargp)
{
    for(int i=0; i<cnt; i++)
    {
	void *dst = NULL;
	int e;
	if (e=pmemobj_zalloc(pop, dst, TOID_TYPE_NUM(uint8_t), len)) {
	    printf("Allocation failed (%d)\n", e);
	    exit(-1);
        }
    }
    return NULL;
}

int
main(int argc, char *argv[])
{
    if (argc != 5) {
        printf("usage: %s [file-name] [block-size] [count/thread] [threads] \n", argv[0]);
        return 1;
    }

    const char *path = argv[1];

    printf("pool file: %s\n", path);

    if (file_exists(path) != 0) {
        if ((pop = pmemobj_create(path, POBJ_LAYOUT_NAME(alloc),
            PMEMOBJ_MIN_POOL, 0666)) == NULL) {
            perror("failed to create pool\n");
            return 1;
        }
    } else {
        printf("pool file not exists\n");
        printf("To create pool run: pmempool create -s 8G obj --layout=alloc path_to_pool\n");
        return 1;
    }
    
    len = atoi(argv[2]);
    cnt = atoi(argv[3]);
    thr = atoi(argv[4]);

    printf("Allocating %d block(s) of %d byte(s) in %d thread(s)\n", cnt*thr, len, thr);

    pthread_t thread_id;
    pthread_t *ids = (pthread_t*)malloc(sizeof(pthread_t)*thr);
    for(int t=0; t<thr; t++) {
        pthread_create(&ids[t], NULL, worker, NULL); 
    }
    for(int t=0; t<thr; t++) {
        pthread_join(ids[t], NULL);
    }

    pmemobj_close(pop);
}