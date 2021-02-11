#include "ex_common.h"
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <libpmemobj.h>

int len = 512;
int cnt = 128;
int thr = 1;
#define REGION_SIZE (8*1024*1024*1024ULL)

#if ALLOC == pmdk

POBJ_LAYOUT_BEGIN(alloc);
POBJ_LAYOUT_TOID(alloc, uint8_t);
POBJ_LAYOUT_END(alloc);

PMEMobjpool *pop;

#define HEAP_FILE "/mnt/pmem0/pmdk.pool"
int pm_init() {
    pop = pmemobj_create(HEAP_FILE, "test", REGION_SIZE, 0666);
    if (pop == nullptr) {
        perror("pmemobj_create");
        return 1;
    }
    return 0;
}

void pm_close() {
    pmemobj_close(pop);
}

void *pm_alloc(size_t len) {
    void *dst = NULL;
    if(pmemobj_zalloc(pop, dst, TOID_TYPE_NUM(uint8_t), len)==0) 
        return dst;
    return NULL;
}

#elif ALLOC == r

int pm_init() {
}

void pm_close() {
}

void *pm_alloc(size_t len) {
}

#else

int pm_init() {}

void pm_close() {}

void *pm_alloc(size_t len) {
    return malloc(len);
}

#endif


void *worker(void *vargp)
{
    for(int i=0; i<cnt; i++)
    {
	void *dst = NULL;
	int e;
	if (e=) {
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
        printf("usage: %s [block-size] [count/thread] [threads] \n", argv[0]);
        return 1;
    }
    
    if (pm_alloc()) {
        return 1;
    }
    
    len = atoi(argv[1]);
    cnt = atoi(argv[2]);
    thr = atoi(argv[3]);

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