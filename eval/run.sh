#!/bin/bash

pool=/mnt/pmem0/pmem.pool
full_path=$(realpath $0)
dir_path=$(dirname $full_path)

source $HOME/.cargo/env
rustup default nightly

p=$(pwd)
cd $dir_path/..
cargo build --release --examples
cd $p

[ -f $dir_path/inputs.tar.gz ] && tar xzvf $dir_path/inputs.tar.gz -C $dir_path && rm -f $dir_path/inputs.tar.gz

ls -1 $dir_path/inputs/wc/* > $dir_path/files.list
mkdir -p $dir_path/outputs/wc

for r in 1 2; do
    for c in 1 2 3 7 14; do
        rm -f $pool
        echo -e "\nRunning scalability test $r:$c ..."
        perf stat -o $dir_path/outputs/wc/$r-$c.out -C 0-$(($r+$c-1)) $dir_path/../target/release/examples/grep -r $r -c $c -f $pool $dir_path/files.list > $dir_path/outputs/wc/$r-$c.res
    done
done

function read_time() {
	echo $(cat $1 | grep -oP '(\d+\.\d+)\s+seconds time elapsed' | grep -oP '(\d+\.\d+)')
}

echo "p/c,1,2,3,7,14," > $dir_path/outputs/scale.csv

for r in 1 2; do
    echo -n "$r,"
    for c in 1 2 3 7 14; do
        echo -n $(read_time "$dir_path/outputs/wc/$r-$c.out"),
    done
echo
done >> $dir_path/outputs/scale.csv

mkdir -p $dir_path/outputs/perf

ins=(INS CHK REM RAND)

rm -f $pool
for i in ${ins[@]}; do
echo "Running performance test (PMDK-B+Tree:$i)..."
PMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=1 PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -C 0 -o $dir_path/outputs/perf/pmdk-$i.out -d $dir_path/pmdk-1.8/src/examples/libpmemobj/map/mapcli btree $pool < $dir_path/inputs/perf/$i > /dev/null
done

rm -f $pool
echo "Running performance test (PMDK-BST:INS)..."
CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=1 PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -C 0 -o $dir_path/outputs/perf/pmdk-bst-INS.out -d $dir_path/bst/btree $pool s 30000
echo "Running performance test (PMDK-BST:CHK)..."
CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=1 PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -C 0 -o $dir_path/outputs/perf/pmdk-bst-CHK.out -d $dir_path/bst/btree $pool r 30000

rm -f $pool
pmempool create obj --layout=simplekv -s 1G $pool
echo "Running performance test (PMDK-KVStore:PUT)..."
CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=1 PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -C 0 -o $dir_path/outputs/perf/pmdk-kv-PUT.out -d $dir_path/simplekv/simplekv $pool burst put 100000
echo "Running performance test (PMDK-KVStore:GET)..."
CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=1 PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -C 0 -o $dir_path/outputs/perf/pmdk-kv-GET.out -d $dir_path/simplekv/simplekv $pool burst get 100000

rm -f $pool
for i in ${ins[@]}; do
echo "Running performance test (Corundum-B+Tree:$i)..."
CPUS=1 perf stat -C 0 -o $dir_path/outputs/perf/crndm-$i.out -d $dir_path/../target/release/examples/mapcli btree $pool < $dir_path/inputs/perf/$i > /dev/null
done

rm -f $pool
echo "Running performance test (Corundum-BST:INS)..."
CPUS=1 perf stat -C 0 -o $dir_path/outputs/perf/crndm-bst-INS.out -d $dir_path/../target/release/examples/btree $pool s 30000
echo "Running performance test (Corundum-BST:CHK)..."
CPUS=1 perf stat -C 0 -o $dir_path/outputs/perf/crndm-bst-CHK.out -d $dir_path/../target/release/examples/btree $pool r 30000

rm -f $pool
echo "Running performance test (Corundum-KVStore:PUT)..."
CPUS=1 perf stat -C 0 -o $dir_path/outputs/perf/crndm-kv-PUT.out -d $dir_path/../target/release/examples/simplekv $pool burst put 100000
echo "Running performance test (Corundum-KVStore:GET)..."
CPUS=1 perf stat -C 0 -o $dir_path/outputs/perf/crndm-kv-GET.out -d $dir_path/../target/release/examples/simplekv $pool burst get 100000

echo "Execution Time (s),,,,,,,,,"                                         > $dir_path/outputs/perf.csv
echo ",BST,,KVStore,,B+Tree,,,,"                                          >> $dir_path/outputs/perf.csv
echo ",INS,CHK,PUT,GET,INS,CHK,REM,RAND"                                  >> $dir_path/outputs/perf.csv
echo -n PMDK,$(read_time "$dir_path/outputs/perf/pmdk-bst-INS.out"),      >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-bst-CHK.out"),      >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-kv-PUT.out"),       >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-kv-GET.out"),       >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-INS.out"),          >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-CHK.out"),          >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-REM.out"),          >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/pmdk-RAND.out")          >> $dir_path/outputs/perf.csv
echo                                                                      >> $dir_path/outputs/perf.csv
echo -n Corundum,$(read_time "$dir_path/outputs/perf/crndm-bst-INS.out"), >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-bst-CHK.out"),     >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-kv-PUT.out"),      >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-kv-GET.out"),      >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-INS.out"),         >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-CHK.out"),         >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-REM.out"),         >> $dir_path/outputs/perf.csv
echo -n      $(read_time "$dir_path/outputs/perf/crndm-RAND.out")         >> $dir_path/outputs/perf.csv
echo                                                                      >> $dir_path/outputs/perf.csv

echo -e "\nPerformance Results"
cat $dir_path/outputs/perf.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s, 


echo -e "\nScalability Results"
cat $dir_path/outputs/scale.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s, 
