#!/bin/bash
  
full_path=$(realpath $0)
dir_path=$(dirname $full_path)

apt -y install scons
apt -y install libconfig-dev libconfig9
apt -y install libelf-dev elfutils
apt -y install libevent-dev
apt -y install libattr1-dev libnuma1 libnuma-dev libyaml-cpp-dev
apt -y install python-dev libxml2-dev libxslt-dev
apt -y install g++-7

mkdir -p $dir_path
cd $dir_path
wget https://sourceforge.net/projects/boost/files/boost/1.62.0/boost_1_62_0.tar.gz
tar xf boost_1_62_0.tar.gz
cd boost_1_62_0
./bootstrap.sh
./b2 -j$((`lscpu -e=cpu | wc -l` - 1)) install

cd $dir_path
git clone https://github.com/snalli/mnemosyne-gcc.git
cd mnemosyne-gcc/usermode/library/pmalloc/include/alps
mkdir build
cd build
cmake .. -DTARGET_ARCH_MEM=CC-NUMA -DCMAKE_BUILD_TYPE=Release
make -j
cd $dir_path/mnemosyne-gcc/usermode
export PYTHONPATH=$dir_path/mnemosyne-gcc/usermode/library/configuration:$PYTHONPATH
export LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH
sed -i "s/\['BUILD_DEBUG'\] = True/\['BUILD_DEBUG'\] = False/g" SConstruct
sed -i "s/'gcc'/'gcc-7'/g" SConstruct
sed -i "s/'g++'/'g++-7'/g" SConstruct
sed -i "s/\-O0/\-O2/g" examples/SConscript
sed -i "s/string.join(DISABLE_WARNINGS, ' ')/' '.join(DISABLE_WARNINGS)/g" library/pmalloc/SConscript
sed -i "s/\-fno\-rtti \-fno\-exceptions //g" library/pmalloc/SConscript
sed -i "s/dev\/shm/mnt\/pmem0/g" mnemosyne.ini
scons

cp -r $dir_path/examples .
scons build --build-example=btree
scons build --build-example=btree_map
scons build --build-example=simplekv

echo "export PYTHONPATH=$dir_path/mnemosyne-gcc/usermode/library/configuration:\$PYTHONPATH
export LD_LIBRARY_PATH=/usr/local/lib:\$LD_LIBRARY_PATH
export PYTHONPATH=`pwd`/library/configuration:\$PYTHONPATH" >> $HOME/.corundum/env