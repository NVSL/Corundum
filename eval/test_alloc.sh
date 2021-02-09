#!/bin/bash

pool=/mnt/pmem0/pmem.pool
full_path=$(realpath $0)
dir_path=$(dirname $full_path)

rm -f $pool
pmempool create -s 8G obj --layout=alloc $pool
cd $dir_path/pmdk
gcc -O2 -o alloc alloc.c -lpmemobj -lpthread

len=512
cnt=1024
thrd=8
nofopt=1

CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 \
    taskset -c 0-$(($thrd-1)) \
    perf stat -o $dir_path/outputs/perf/pmdk-kv-GET.out -d \
    alloc $pool $len $cnt $thrd