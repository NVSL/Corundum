#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

cd $dir_path
git clone https://github.com/HewlettPackard/Atlas.git
cp -r deltas/* Atlas/
cd Atlas/compiler-plugin
./build_plugin
cd ../runtime
mkdir build
cd build
cmake -D CMAKE_BUILD_TYPE=Release .. && make -j