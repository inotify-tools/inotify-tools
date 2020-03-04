#!/bin/bash

set -e

j=3

printf "gcc build\n"
git clean -fdx 2>&1
export CC=gcc
./autogen.sh
./configure
make -j$j

os=$(uname -o)

if [ "$os" != "FreeBSD" ]; then
  printf "\nunit test\n"
  cd libinotifytools/src/
  make -j$j test
  ./test
  cd -
fi

printf "\nintegration test\n"
cd t
make -j$j
cd -

printf "\nclang build\n"
git clean -fdx 2>&1
export CC=clang
./autogen.sh
./configure
make -j$j

if [ "$os" != "FreeBSD" ]; then
  printf "\nunit test\n"
  cd libinotifytools/src/
  make -j$j test
  ./test
  cd -
fi

printf "\nintegration test\n"
cd t
make -j$j

