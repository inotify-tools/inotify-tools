#!/bin/sh

set -ex

j=16

unit_test() {
  if [ "$os" != "freebsd" ]; then
    printf "\nunit test\n"
    cd libinotifytools/src/
    make -j$j test
    ./test
    cd -
  fi
}

integration_test() {
  printf "\nintegration test\n"
  for i in {1..128}; do
    cd t
    make -j$j
    cd -
  done
}

tests() {
  unit_test
  integration_test
}

clean() {
  make distclean || true
  if [ "$arg1" = "clean" ]; then
    git clean -fdx > /dev/null 2>&1
  fi
}

build() {
  ./autogen.sh
  ./configure --prefix=/usr $@
  make -j$j
  unset CFLAGS
  unset CXXFLAGS
  unset LDFLAGS
}

set_gcc() {
  export CC=gcc
  export CXX=g++
}

set_clang() {
  export CC=clang
  export CXX=clang++
}

arg1="$1"

os=$(uname -o | sed "s#GNU/##g" | tr '[:upper:]' '[:lower:]')
uname_m=$(uname -m)

if command -v sudo; then
  pre="sudo"
fi

if command -v apt; then
  $pre apt update || true
  $pre apt install -y gcc-arm-linux-gnueabihf || true
  $pre apt install -y g++-arm-linux-gnueabihf || true
  $pre apt install -y clang || true
  $pre apt install -y gcc || true
  $pre apt install -y g++ || true
  $pre apt install -y clang-tidy || true
  $pre apt install -y clang-format || true
  $pre apt install -y clang-tools || true
  $pre apt install -y clang-format-11 || true
  $pre apt install -y doxygen || true
  $pre apt install -y make || true
  $pre apt install -y autoconf || true
  $pre apt install -y libtool || true
  $pre apt install -y curl || true
  $pre apt install -y git || true
  $pre apt install -y zip || true
elif command -v apk; then
  apk add build-base alpine-sdk autoconf automake libtool bash coreutils clang \
    clang-extra-tools lld linux-headers curl git zip gcompat
fi

#!/bin/bash

set -e

for i in {64..11}; do
  if command -v "git-clang-format-$i" > /dev/null; then
    CLANG_FMT_VER="clang-format-$i"
    break
  fi
done

if [ -n "$CLANG_FMT_VER" ]; then
  printf "\nclang-format build\n"
  if ! git $CLANG_FMT_VER HEAD^ | grep -q "modif"; then
    echo -e "\nPlease change style to the format defined in the" \
            ".clang-format file:\n"
    git diff --name-only
    exit 1
  fi
fi

printf "gcc build\n"
clean
set_gcc
build
tests

usr_inc="/usr/include"
inotifytools_inc="libinotifytools/src"
inotifytools_inc2="$inotifytools_inc/inotifytools"
inc="-I$usr_inc -I$inotifytools_inc -I$inotifytools_inc2"

if command -v doxygen > /dev/null; then
  printf "rh build\n"
  clean
  set_gcc
  ./rh_build.sh
  tests
fi

printf "gcc static build\n"
clean
set_gcc
build --enable-static --disable-shared
tests

if [ "$os" != "freebsd" ] && ldconfig -p | grep -q libasan; then
  printf "\ngcc address sanitizer build\n"
  clean
  set_gcc
  export CFLAGS="-fsanitize=address -O0 -ggdb"
  export CXXFLAGS="-fsanitize=address -O0 -ggdb"
  export LDFLAGS="-fsanitize=address -O0 -ggdb"
  build
  tests
fi

if command -v arm-linux-gnueabihf-gcc > /dev/null; then
  printf "\ngcc arm32 build\n"
  clean
  export CC="arm-linux-gnueabihf-gcc"
  export CXX="arm-linux-gnueabihf-g++"
  build --host=arm-linux-gnueabihf
  if [ "$uname_m" == "aarch64" ]; then
    tests
  fi
fi

printf "\nclang build\n"
clean
set_clang
build
tests

if command -v scan-build > /dev/null; then
  printf "\ngcc scan-build\n"
  clean

  set_gcc
  scan-build ./autogen.sh
  scan-build ./configure
  scan_build_args="-disable-checker unix.Malloc"
  scan_build_args="$scan_build_args -disable-checker core.AttributeNonNull"
  scan_build_args="$scan_build_args -disable-checker core.NonNullParamChecker"
  scan_build="$(scan-build $scan_build_args make -j$j)"
  echo "$scan_build"
  if ! echo "$scan_build" | grep -qi "no bugs found\|0 bugs found"; then
    false
  fi

  printf "\nclang scan-build\n"
  clean

  set_clang
  scan-build ./autogen.sh
  scan-build ./configure
  scan_build=$(scan-build $scan_build_args make -j$j)
  echo "$scan_build"
  if ! echo "$scan_build" | grep -qi "no bugs found\|0 bugs found"; then
    false
  fi
fi

tests

printf "\nclang static build\n"
clean
set_clang
build --enable-static --disable-shared
tests

printf "\ngcc coverage build\n"
clean
set_gcc
export CFLAGS="--coverage"
export CXXFLAGS="--coverage"
export LDFLAGS="--coverage"
build --enable-static --disable-shared
tests

curl -s https://codecov.io/bash | /bin/bash

if [ "$os" != "freebsd" ] && [ "$(uname -m)" = "x86_64" ]; then
  id=$(grep ^ID= /etc/os-release | sed "s/^ID=//g")

  # Don't do sonarcloud on alpine
  if [ "$id" = "alpine" ]; then
    exit 0
  fi

  printf "\nsonar build\n"

  # sonarcloud
  export SONAR_TOKEN="0bc5d48614caa711d6b908f80c039464aff99611"
  mkdir -p $HOME/.sonar
  SONAR_SCANNER_VERSION="4.4.0.2170"
  SONAR_SCANNER_DOWNLOAD_URL="https://binaries.sonarsource.com/Distribution/sonar-scanner-cli/sonar-scanner-cli-$SONAR_SCANNER_VERSION-linux.zip"
  curl -sSLo $HOME/.sonar/sonar-scanner.zip $SONAR_SCANNER_DOWNLOAD_URL
  unzip -o $HOME/.sonar/sonar-scanner.zip -d $HOME/.sonar/
  PATH="$HOME/.sonar/sonar-scanner-$SONAR_SCANNER_VERSION-linux/bin:$PATH"
  SONAR_SERVER_URL="https://sonarcloud.io"
  BUILD_WRAPPER_DOWNLOAD_URL="$SONAR_SERVER_URL/static/cpp/build-wrapper-linux-x86.zip"
  curl -sSLo $HOME/.sonar/build-wrapper-linux-x86.zip $BUILD_WRAPPER_DOWNLOAD_URL
  unzip -o $HOME/.sonar/build-wrapper-linux-x86.zip -d $HOME/.sonar/
  PATH="$HOME/.sonar/build-wrapper-linux-x86:$PATH"
  BUILD_WRAPPER_OUT_DIR="build_wrapper_output_directory"
  clean
  set_gcc
  unset CFLAGS
  unset CXXFLAGS
  unset LDFLAGS
  ./autogen.sh
  ./configure
  build-wrapper-linux-x86-64 --out-dir $BUILD_WRAPPER_OUT_DIR make -j$j
  sonar-scanner --define sonar.host.url="$SONAR_SERVER_URL" --define sonar.cfamily.build-wrapper-output="$BUILD_WRAPPER_OUT_DIR"
fi

