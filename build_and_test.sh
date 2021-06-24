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

os=$(uname -o | sed "s#GNU/##g" | tr '[:upper:]' '[:lower:]')

if [ "$os" != "freebsd" ]; then
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

if [ "$os" != "freebsd" ]; then
  printf "\nunit test\n"
  cd libinotifytools/src/
  make -j$j test
  ./test
  cd -
fi

integration_test

if [ "$os" != "freebsd" ]; then
  printf "gcc address sanitizer build\n"
  make distclean
  if [ "$1" == "clean" ]; then
    git clean -fdx 2>&1
  fi

  ./autogen.sh
  ./configure CFLAGS="-fsanitize=address -O0 -ggdb" \
    LDFLAGS="-fsanitize=address -O0 -ggdb"
  make -j$j

  printf "\nunit test\n"
  cd libinotifytools/src/
  make -j$j test
  ./test
  cd -

  integration_test
fi

printf "\nclang build\n"
make distclean
if [ "$1" == "clean" ]; then
  git clean -fdx 2>&1
fi

export CC=clang
./autogen.sh
./configure
make -j$j

if [ "$os" != "freebsd" ]; then
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

if [ "$os" != "freebsd" ]; then
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

  if [ "$1" == "clean" ]; then
    git clean -fdx 2>&1
  fi

  export CC=gcc

  if [ "$os" != "freebsd" ]; then
    file="/tmp/cov-analysis-${os}64.tar.gz"
    project="inotifytools"
    token="Dy7fkaSpHHjTg8JMFHKgOw"
    curl -o "$file" https://scan.coverity.com/download/${os}64 \
      --form project="$project" --form token="$token"
    tar xf "$file"
    ./autogen.sh
    ./configure
    cov-analysis-${os}64-*/bin/cov-build --dir cov-int make -j$j
    tar cfz cov-int.tar.gz cov-int
    version="$(git rev-parse HEAD)"
    description="$(git show --no-patch --oneline)"
    curl --form token="$token" \
      --form email=ericcurtin17@gmail.com \
      --form file=@cov-int.tar.gz \
      --form version="$version" \
      --form description="$description" \
      https://scan.coverity.com/builds?project=$project
  fi
fi

