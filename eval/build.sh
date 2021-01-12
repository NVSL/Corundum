#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

rm -f $HOME/.corundum/env

$dir_path/pmdk/build.sh
$dir_path/atlas/build.sh
$dir_path/go/build.sh
$dir_path/mnemosyne/build.sh

source $HOME/.cargo/env
rustup default nightly
rustup update

mkdir $HOME/.corundum
echo "source \$HOME/.cargo/env" >> $HOME/.corundum/env

cd $dir_path/..
cargo build --release --examples

echo "Please run the following command to setup the environment:
    source \$HOME/.corundum/env"

