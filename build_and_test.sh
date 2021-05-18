#!/bin/bash

set -e

j=16

integration_test() {
  printf "\nintegration test\n"
  for i in {1..128}; do
    cd t
    make -j$j
    cd -
  done
}


printf "gcc build\n"
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

integration_test

printf "gcc static build\n"
make distclean
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

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

integration_test

printf "\nclang build\n"
make distclean
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

integration_test

printf "\nclang static build\n"
make distclean
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

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

integration_test


if [ -n "$TRAVIS" ] || [ -n "$CI" ]; then
  for i in {64..8}; do
    if command -v "git-clang-format-$i"; then
      CLANG_FMT_VER="clang-format-$i"
      break
    fi
  done

  if [ -n "$CLANG_FMT_VER" ]; then
    if ! git $CLANG_FMT_VER HEAD^ | grep -q "modif"; then
      echo -e "\nPlease change style to the format defined in the" \
              ".clang-format file:\n"
      git diff --name-only
      exit 1
    fi
  fi
fi

