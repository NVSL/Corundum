#!/bin/bash

for i in `seq 470 703`; do
    rm -f buddy.pool
    CRASH_AT=$i cargo test buddy_alg_test -- --nocapture
    cargo test buddy_alg_test -- --nocapture
    if [ $? -eq 0 ]; then 
        echo "test $i   [ok]"
    else
        echo "test $i   [failed]"
        break
    fi
    res=
done