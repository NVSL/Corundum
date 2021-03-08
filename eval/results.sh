#!/bin/bash

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
    echo "    -h, --help            Display this information"
}

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
        --*)                 echo "bad option $1"
            ;;
        *)                   echo "argument $1"
            ;;
    esac
    shift
done

function read_time() {
    echo $(cat $1 | grep -oP '(\d+\.\d+)\s+seconds time elapsed' | grep -oP '(\d+\.\d+)')
}

echo ",Execution Time (s),,,,,,,,"                                              > $dir_path/outputs/perf.csv
echo ",BST,,KVStore,,B+Tree,,,,"                                               >> $dir_path/outputs/perf.csv
echo ",INS,CHK,PUT,GET,INS,CHK,REM,RAND"                                       >> $dir_path/outputs/perf.csv

if $all || $pmdk; then
    echo -n PMDK,$(read_time "$dir_path/outputs/perf/pmdk-bst-INS.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-bst-CHK.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-kv-PUT.out"),            >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-kv-GET.out"),            >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-INS.out"),               >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-CHK.out"),               >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-REM.out"),               >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/pmdk-RAND.out")               >> $dir_path/outputs/perf.csv
    echo                                                                           >> $dir_path/outputs/perf.csv
fi

if $all || $atlas; then
    echo -n Atlas,$(read_time "$dir_path/outputs/perf/atlas-bst-INS.out"),         >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-bst-CHK.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-kv-PUT.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-kv-GET.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-INS.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-CHK.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-REM.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/atlas-RAND.out")              >> $dir_path/outputs/perf.csv
    echo                                                                           >> $dir_path/outputs/perf.csv
fi

if $all || $mnemosyne; then
    echo -n Mnemosyne,$(read_time "$dir_path/outputs/perf/mnemosyne-bst-INS.out"), >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-bst-CHK.out"),      >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-kv-PUT.out"),       >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-kv-GET.out"),       >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-INS.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-CHK.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-REM.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/mnemosyne-RAND.out")          >> $dir_path/outputs/perf.csv
    echo                                                                           >> $dir_path/outputs/perf.csv
fi

if $all || $go; then
    echo -n go-pmem,$(read_time "$dir_path/outputs/perf/go-bst-INS.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-bst-CHK.out"),             >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-kv-PUT.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-kv-GET.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-INS.out"),                 >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-CHK.out"),                 >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-REM.out"),                 >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/go-RAND.out")                 >> $dir_path/outputs/perf.csv
    echo                                                                           >> $dir_path/outputs/perf.csv
fi

if $all || $crndm; then
    echo -n Corundum,$(read_time "$dir_path/outputs/perf/crndm-bst-INS.out"),      >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-bst-CHK.out"),          >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-kv-PUT.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-kv-GET.out"),           >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-INS.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-CHK.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-REM.out"),              >> $dir_path/outputs/perf.csv
    echo -n      $(read_time "$dir_path/outputs/perf/crndm-RAND.out")              >> $dir_path/outputs/perf.csv
    echo                                                                           >> $dir_path/outputs/perf.csv
fi

if $all || $scale; then
    rs=(1)
    cs=`seq 0 15`
    
    echo -n "p/c," > $dir_path/outputs/scale.csv
    (for c in ${cs[@]}; do
        echo -n "$c,"
    done; echo) >> $dir_path/outputs/scale.csv

    b=$(read_time "$dir_path/outputs/wc/1-0.out")
    for r in ${rs[@]}; do
        echo -n "p=$r,"
        for c in ${cs[@]}; do
            m=$(read_time "$dir_path/outputs/wc/$r-$c.out")
            g=$(echo - | awk "{print $b / $m}")
            echo -n $g,
        done
        echo
    done >> $dir_path/outputs/scale.csv
fi

function avg() {
    echo $(cat $1 | grep -oP "$2 .+avg\\(ns\\): \\d+\\.\\d{3} " | grep -oP '(\d+\.\d{3}) ')
}

function std() {
    echo $(cat $1 | grep -oP "$2 .+std\\(ns\\): \\d+\\.\\d |$2 .+std\\(ns\\): NaN" | grep -oP '(\d+\.\d) |(NaN)')
}

tags=(
    "Deref"
    "DerefMut\(1st\)"
    "DerefMut\(!1st\)"
    "Alloc\(8\)"
    "Alloc\(256\)"
    "Alloc\(4096\)"
    "Pbox:AtomicInit"
    "Prc:AtomicInit"
    "Parc:AtomicInit"
    "Dealloc\(8\)"
    "Dealloc\(256\)"
    "Dealloc\(4096\)"
    "TxNop"
    "DataLog\(8\)"
    "DataLog\(1024\)"
    "DataLog\(4096\)"
    "DropLog\(8\)"
    "DropLog\(32768\)"
    "Pbox:clone"
    "Prc:clone"
    "Parc:clone"
    "Prc:downgrade"
    "Parc:downgrade"
    "Prc:upgrade"
    "Parc:upgrade"
    "Prc:demote"
    "Parc:demote"
    "Prc:promote"
    "Parc:promote"
)

if $all || $micro; then
    p=$dir_path/outputs/perf/micro-pmem.out
    d=$dir_path/outputs/perf/micro-dram.out
    m=$dir_path/outputs/micro.csv
    echo ",PMEM,,DRAM," > $m
    echo ",Mean (ns),STD (ns),Mean (ns),STD (ns)" >> $m
    for t in ${tags[@]}; do
        echo "${t//\\/},$(avg $p $t),$(std $p $t),$(avg $d $t),$(std $d $t)" >> $m
    done 
fi

if $all || $pmdk || $atlas || $go || $micro || $mnemosyne || $crndm; then
    echo -e "\nPerformance Results"
    cat $dir_path/outputs/perf.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s,
fi

if $all || $scale; then
    echo -e "\nScalability Results"
    cat $dir_path/outputs/scale.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s,
fi

if $all || $micro; then
    echo -e "\nBasic Operation Latency"
    cat $dir_path/outputs/micro.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s,
fi