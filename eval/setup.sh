#!/bin/bash

apt-get update

apt-get -y install wget
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
apt-get -y install doxygen pandoc bsdmainutils
apt-get -y install linux-tools-generic linux-cloud-tools-generic
apt-get -y install llvm clang cmake libboost-graph-dev
apt-get -y install golang

rm -f /usr/bin/perf
ln -s /usr/lib/linux-tools/*/perf /usr/bin/perf

wget https://github.com/NVSL/Corundum/raw/24130f8789b4bed6cf6526562045586e19e88592/eval/inputs.tar.gz

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
rustup default nightly

