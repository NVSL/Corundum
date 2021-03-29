pool=/mnt/pmem0/pmem.pool
full_path=$(realpath $0)
dir_path=$(dirname $full_path)

all=true
scale=false
pmdk=false
atlas=false
go=false
micro=false
mnemosyne=false
crndm=false
nofopt=1

function help() {
    echo "usage: $0 [OPTIONS]"
    echo "OPTIONS:"
    echo "    -s, --scale           Test scalability (imperfect isolation)"
    echo "    -p, --pmdk            Run PMDK performance tests"
    echo "    -a, --atlas           Run Atlas performance tests"
    echo "    -g, --go-pmem         Run go-pmem performance tests"
    echo "    -m, --mnemosyne       Run Mnemosyne performance tests"
    echo "    -c, --corundum        Run Corundum performance tests"
    echo "    -M, --micro-bench     Run Corundum basic operation latency measurement"
    echo "    -n, --no-run          Do not run the experiments"
    # echo "    -j, --pin-journals    Enable 'pin_journal' feature in Corundum"
    echo "    -o, --clflushopt      Allow using CLFLUSHOPT"
    echo "    -h, --help            Display this information"
}

features="pin_journals,$FEATURES"
while test $# -gt 0
do
    case "$1" in
        -h|--help)           help && exit 0
            ;;
        -s|--scale)          all=false && scale=true
            ;;
        -p|--pmdk)           all=false && pmdk=true
            ;;
        -a|--atlas)          all=false && atlas=true
            ;;
        -g|--go-pmem)        all=false && go=true
            ;;
        -c|--corundum)       all=false && crndm=true
            ;;
        -m|--mnemosyne)      all=false && mnemosyne=true
            ;;
        -M|--micro-bench)    all=false && micro=true
            ;;
        -n|--no-run)         all=false
            ;;
        -j|--pin-journals)   features="pin_journals,$features"
            ;;
        -o|--clflushopt)     nofopt=0 && features="use_clflushopt,$features"
            ;;
        --*)                 echo "bad option $1"
            ;;
        *)                   echo "argument $1"
            ;;
    esac
    shift
done

source $HOME/.corundum/env
rustup default nightly

[ -f $dir_path/inputs.tar.gz ] || [ -d $dir_path/inputs ] || \
    wget https://github.com/NVSL/Corundum/raw/24130f8789b4bed6cf6526562045586e19e88592/eval/inputs.tar.gz

[ -f $dir_path/inputs.tar.gz ] && \
    tar xzvf $dir_path/inputs.tar.gz -C $dir_path && \
    rm -f $dir_path/inputs.tar.gz && \
    for i in `ls $dir_path/inputs/wc/*`; do cp $i $i-1; done

ls -1 $dir_path/inputs/wc/* > $dir_path/files.list
mkdir -p $dir_path/outputs/wc
mkdir -p $dir_path/outputs/perf

rs=(1)
cs=`seq 0 15`
if $all || $scale; then
    cd $dir_path/..
    cargo build --release --example grep --features="$features"

    for r in ${rs[@]}; do
        for c in ${cs[@]}; do
            rm -f $pool
            echo -e "\nRunning scalability test $r:$c ..."
            CPUS=$(($r+$c)) perf stat -o $dir_path/outputs/wc/$r-$c.out -a -C 0-$(($r+$c-1)) taskset -c 0-$(($r+$c-1)) $dir_path/../target/release/examples/grep -N -r $r -c $c -f $pool $dir_path/files.list > $dir_path/outputs/wc/$r-$c.res
        done
    done
    echo
fi

ins=(INS CHK REM RAND)

if $all || $pmdk; then
    rm -f $pool
    echo "Running performance test (PMDK-BST:INS)..."
    CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -o $dir_path/outputs/perf/pmdk-bst-INS.out -d $dir_path/pmdk/btree $pool s 30000
    echo "Running performance test (PMDK-BST:CHK)..."
    CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -o $dir_path/outputs/perf/pmdk-bst-CHK.out -d $dir_path/pmdk/btree $pool r 30000

    rm -f $pool
    pmempool create obj --layout=simplekv -s 1G $pool
    echo "Running performance test (PMDK-KVStore:PUT)..."
    CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -o $dir_path/outputs/perf/pmdk-kv-PUT.out -d $dir_path/pmdk/simplekv $pool burst put 65536
    echo "Running performance test (PMDK-KVStore:GET)..."
    CPMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -o $dir_path/outputs/perf/pmdk-kv-GET.out -d $dir_path/pmdk/simplekv $pool burst get 65536

    rm -f $pool
    for i in ${ins[@]}; do
    echo "Running performance test (PMDK-B+Tree:$i)..."
    PMEM_NO_CLWB=1 PMEM_NO_CLFLUSHOPT=$nofopt PMEM_NO_MOVNT=1 PMEM_NO_FLUSH=0 perf stat -o $dir_path/outputs/perf/pmdk-$i.out -d $dir_path/pmdk/pmdk-1.8/src/examples/libpmemobj/map/mapcli btree $pool < $dir_path/inputs/perf/$i > /dev/null
    done
fi

if $all || $atlas; then
    rm -rf /mnt/pmem0/`whoami`
    echo "Running performance test (Atlas-BST:INS)..."
    perf stat -o $dir_path/outputs/perf/atlas-bst-INS.out -d $dir_path/atlas/Atlas/runtime/build/tests/data_structures/btree s 30000
    echo "Running performance test (Atlas-BST:CHK)..."
    perf stat -o $dir_path/outputs/perf/atlas-bst-CHK.out -d $dir_path/atlas/Atlas/runtime/build/tests/data_structures/btree r 30000

    echo "Running performance test (Atlas-KVStore:PUT)..."
    perf stat -o $dir_path/outputs/perf/atlas-kv-PUT.out -d $dir_path/atlas/Atlas/runtime/build/tests/data_structures/simplekv burst put 65536
    echo "Running performance test (Atlas-KVStore:GET)..."
    perf stat -o $dir_path/outputs/perf/atlas-kv-GET.out -d $dir_path/atlas/Atlas/runtime/build/tests/data_structures/simplekv burst get 65536

    rm -rf /mnt/pmem0/`whoami`  # Static in the code
    for i in ${ins[@]}; do
        echo "Running performance test (Atlas-B+Tree:$i)..."
        perf stat -C 0-4 -o $dir_path/outputs/perf/atlas-$i.out -d $dir_path/atlas/Atlas/runtime/build/tests/data_structures/btree_map < $dir_path/inputs/perf/$i > /dev/null
    done
fi

if $all || $mnemosyne; then
    cd $dir_path/mnemosyne/mnemosyne-gcc/usermode
    rm -rf /mnt/pmem0/psegments
    echo "Running performance test (Mnemosyne-BST:INS)..."
    perf stat -o $dir_path/outputs/perf/mnemosyne-bst-INS.out -d ./build/examples/btree/btree s 30000
    echo "Running performance test (Mnemosyne-BST:CHK)..."
    perf stat -o $dir_path/outputs/perf/mnemosyne-bst-CHK.out -d ./build/examples/btree/btree r 30000

    echo "Running performance test (Mnemosyne-KVStore:PUT)..."
    perf stat -o $dir_path/outputs/perf/mnemosyne-kv-PUT.out -d ./build/examples/simplekv/simplekv burst put 65536
    echo "Running performance test (Mnemosyne-KVStore:GET)..."
    perf stat -o $dir_path/outputs/perf/mnemosyne-kv-GET.out -d ./build/examples/simplekv/simplekv burst get 65536

    rm -rf /mnt/pmem0/psegments
    for i in ${ins[@]}; do
        echo "Running performance test (Mnemosyne-B+Tree:$i)..."
        perf stat -a -C 0-4 -o $dir_path/outputs/perf/mnemosyne-$i.out -d ./build/examples/btree_map/btree_map < $dir_path/inputs/perf/$i > /dev/null
    done
    cd $dir_path
fi

if $all || $go; then
    rm -f $pool
    echo "Running performance test (go-pmem-BST:INS)..."
    perf stat -o $dir_path/outputs/perf/go-bst-INS.out -d $dir_path/go/btree $pool s 30000
    echo "Running performance test (go-pmem-BST:CHK)..."
    perf stat -o $dir_path/outputs/perf/go-bst-CHK.out -d $dir_path/go/btree $pool r 30000

    rm -f $pool
    echo "Running performance test (go-pmem-KVStore:PUT)..."
    perf stat -o $dir_path/outputs/perf/go-kv-PUT.out -d $dir_path/go/simplekv $pool burst put 65536
    echo "Running performance test (go-pmem-KVStore:GET)..."
    perf stat -o $dir_path/outputs/perf/go-kv-GET.out -d $dir_path/go/simplekv $pool burst get 65536

    rm -f $pool
    for i in ${ins[@]}; do
    echo "Running performance test (go-pmem-B+Tree:$i)..."
    perf stat -o $dir_path/outputs/perf/go-$i.out -d $dir_path/go/btree_map $pool < $dir_path/inputs/perf/$i > /dev/null
    done
fi

if $all || $crndm; then
    cd $dir_path/..
    cargo build --release --example btree --example mapcli --example simplekv --features="$features"

    rm -f $pool
    echo "Running performance test (Corundum-BST:INS)..."
    CPUS=1 perf stat -o $dir_path/outputs/perf/crndm-bst-INS.out -d $dir_path/../target/release/examples/btree $pool s 30000
    echo "Running performance test (Corundum-BST:CHK)..."
    CPUS=1 perf stat -o $dir_path/outputs/perf/crndm-bst-CHK.out -d $dir_path/../target/release/examples/btree $pool r 30000

    rm -f $pool
    echo "Running performance test (Corundum-KVStore:PUT)..."
    CPUS=1 perf stat -o $dir_path/outputs/perf/crndm-kv-PUT.out -d $dir_path/../target/release/examples/simplekv $pool burst put 65536
    echo "Running performance test (Corundum-KVStore:GET)..."
    CPUS=1 perf stat -o $dir_path/outputs/perf/crndm-kv-GET.out -d $dir_path/../target/release/examples/simplekv $pool burst get 65536

    rm -f $pool
    for i in ${ins[@]}; do
        echo "Running performance test (Corundum-B+Tree:$i)..."
        perf stat -o $dir_path/outputs/perf/crndm-$i.out -d $dir_path/../target/release/examples/mapcli btree $pool < $dir_path/inputs/perf/$i > /dev/null
    done
fi

if $all || $micro; then
    cd $dir_path/..
    rm -f $pool
    echo "Running microbenchmarks on PMEM ($pool)..."
    CPUS=1 taskset -c 0 cargo run --release --example microbench --features="$features" -- $pool > $dir_path/outputs/perf/micro-pmem.out
    echo "Running microbenchmarks on PMEM (/dev/shm/m.pool)..."
    rm -f /dev/shm/m.pool
    CPUS=1 taskset -c 0 cargo run --release --example microbench --features="$features" -- /dev/shm/m.pool > $dir_path/outputs/perf/micro-dram.out
fi