#!/bin/bash

apt-get -y install git-core
apt-get -y install numactl
apt-get -y install build-essential
apt-get -y install uuid-dev
apt-get -y install pkg-config
apt-get -y install libndctl-dev
apt-get -y install libdaxctl-dev
apt-get -y install autoconf
apt-get -y install cmake
apt-get -y install python
apt-get -y install curl
apt-get -y install libz-dev
apt-get -y install doxygen
apt-get -y install libpmem-devel libpmemobj-devel libpmemobj++-devel
apt-get -y install linux-tools-generic linux-cloud-tools-generic linux-tool

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source \$HOME/.cargo/env
rustup default nightly

wget https://github.com/pmem/pmdk/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz
cd pmdk-1.8
cp -f ../bst/btree.c src/examples/libpmemobj/ && make -j || exit 1
sudo make install && cd .. || exit 1

wget https://github.com/pmem/libpmemobj-cpp/archive/1.8.tar.gz && \
    tar -xzvf 1.8.tar.gz && rm -f 1.8.tar.gz
cd libpmemobj-cpp-1.8
cp -f ../simplekv/* examples/simplekv/
mkdir build && cd build && cmake .. && make -j || exit 1
sudo make install && cd ../.. || exit 1

exit 0
