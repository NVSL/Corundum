#!/bin/bash

n=`awk -F= '/^NAME/{print $2}' /etc/os-release`
if [ "$n" != "\"Ubuntu\"" ] && [ "$n" != "Ubuntu" ]; then
  echo "This script does not work on $n (only Ubuntu is supported)."
  exit 1
fi

su=
if [ "$EUID" -ne 0 ]; then
  su=sudo
fi

$su apt-get update

# PMDK's and Atlas's dependencies
$su apt-get -y install wget
$su apt-get -y install git-core
$su apt-get -y install numactl
$su apt-get -y install build-essential
$su apt-get -y install uuid-dev
$su apt-get -y install pkg-config
$su apt-get -y install libndctl-dev
$su apt-get -y install libdaxctl-dev
$su apt-get -y install autoconf
$su apt-get -y install cmake
$su apt-get -y install python
$su apt-get -y install curl
$su apt-get -y install libz-dev
$su apt-get -y install doxygen pandoc bsdmainutils
$su apt-get -y install llvm clang cmake libboost-graph-dev

# go-pmem's dependencies
$su apt-get -y install golang

# Mnemmosyne's dependencies
$su apt-get -y install scons
$su apt-get -y install libconfig-dev libconfig9
$su apt-get -y install libelf-dev elfutils
$su apt-get -y install libevent-dev
$su apt-get -y install libattr1-dev libnuma1 libnuma-dev libyaml-cpp-dev
$su apt-get -y install python-dev libxml2-dev libxslt-dev
$su apt-get -y install g++-7

if ! which perf; then
  if $su apt-get -y install linux-tools-generic linux-cloud-tools-generic; then
    [ -f /usr/bin/perf ] && $su mv /usr/bin/perf /usr/bin/perf.bkup
    $su ln -s /usr/lib/linux-tools/*/perf /usr/bin/perf
  else
    git clone --depth 1 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git
    cd linux/tools/perf
    make && $su rm -f /usr/bin/perf && $su cp perf /usr/bin
    cd ../../..
  fi
fi

# Corundum's dependency
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
$HOME/.cargo/bin/rustup default nightly

