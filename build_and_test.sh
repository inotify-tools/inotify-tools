#!/bin/bash

set -e

j=10

printf "gcc shared build\n"
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

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

printf "\ngcc static build\n"
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

export CC=gcc
./autogen.sh
./configure --enable-static --disable-shared
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

printf "\nclang shared build\n"
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

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

printf "\nclang static build\n"
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

export CC=clang
./autogen.sh
./configure --enable-static --disable-shared
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
