#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

su=
if [ "$EUID" -ne 0 ]; then
  su=sudo
fi

cd $dir_path
wget https://github.com/pmem/pmdk/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz && cd pmdk-1.8 && \
    make -j$(nproc) && $su make install

cd $dir_path
wget https://github.com/pmem/libpmemobj-cpp/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz && cd libpmemobj-cpp-1.8 && \
    mkdir -p build && cd build && cmake -D CMAKE_BUILD_TYPE=Release .. && \
    make -j$(nproc) && $su make install

cd $dir_path
g++ -O2 -o simplekv simplekv.cpp -lpmemobj
gcc -O2 -o btree btree.c -lpmemobj

$su ldconfig