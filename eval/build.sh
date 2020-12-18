#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

wget https://github.com/pmem/pmdk/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz && cd pmdk-1.8 && \
    make -j && make install && cd ..

wget https://github.com/pmem/libpmemobj-cpp/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz && cd libpmemobj-cpp-1.8 && \
    mkdir -p build && cd build && cmake .. && make -j && make install && \
    cd ../..

source $HOME/.cargo/env
rustup update
rustup default nightly

ldconfig

cd $dir_path/simplekv
g++ -O2 -o simplekv simplekv.cpp -lpmemobj

cd $dir_path/bst
gcc -O2 -o btree btree.c -lpmemobj

cd $dir_path/..
cargo build --release --examples

