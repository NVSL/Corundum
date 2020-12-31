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

cd $dir_path
git clone https://github.com/HewlettPackard/Atlas.git
cp -r atlas-deltas/* Atlas/
cd Atlas/compiler-plugin
./build_plugin
cd ../runtime
mkdir build
cd build
cmake -D CMAKE_BUILD_TYPE=Release .. && make -j

cd $dir_path
git clone https://github.com/jerrinsg/go-pmem.git
cd go-pmem/src
./all.bash
apt -y remove golang
apt -y autoremove
echo "export PATH=$dir_path/go-pmem/bin:\$PATH" >> ~/.profile
. ~/.profile
go get -u github.com/vmware/go-pmem-transaction
cd $dir_path/go
go build -txn btree.go
go build -txn btree_map.go
go build -txn simplekv.go

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

