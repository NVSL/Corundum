#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

su=
if [ "$EUID" -ne 0 ]; then
  su=sudo
fi

cd $dir_path
git clone https://github.com/jerrinsg/go-pmem.git
cd go-pmem/src
./make.bash
# $su apt -y remove golang
# $su apt -y autoremove
echo "export PATH=$dir_path/go-pmem/bin:\$PATH" >> $HOME/.corundum/env
source $HOME/.corundum/env
export GO111MODULE=off
go get -u github.com/vmware/go-pmem-transaction
cd $dir_path
go build -txn btree.go
go build -txn btree_map.go
go build -txn simplekv.go