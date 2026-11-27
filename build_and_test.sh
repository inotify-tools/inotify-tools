#!/bin/bash

set -e

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
  if [ "$arg1" == "clean" ]; then
    git clean -fdx > /dev/null 2>&1
  fi
}

build() {
  ./autogen.sh
  ./configure --prefix=/usr $@
  make -j$j
  unset CFLAGS
  unset LDFLAGS
}

arg1="$1"

os=$(uname -o | sed "s#GNU/##g" | tr '[:upper:]' '[:lower:]')
uname_m=$(uname -m)

printf "gcc build\n"
clean
export CC="gcc"
build
tests

if [ -n "$TRAVIS" ] || [ -n "$CI" ]; then
  if [ "$os" != "freebsd" ]; then
    sudo apt update || true
    sudo apt install -y gcc-arm-linux-gnueabihf || true
    sudo apt install -y cppcheck || true
    sudo apt install -y clang || true
    sudo apt install -y gcc || true
    sudo apt install -y clang-tidy || true
    sudo apt install -y clang-format || true
    sudo apt install -y clang-tools || true
    sudo apt install -y clang-format-11 || true
    sudo apt install -y doxygen || true
  fi

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
fi

usr_inc="/usr/include"
inotifytools_inc="libinotifytools/src"
inotifytools_inc2="$inotifytools_inc/inotifytools"
inc="-I$usr_inc -I$inotifytools_inc -I$inotifytools_inc2"

if command -v clang-tidy > /dev/null; then
  printf "\nclang-tidy build\n"
  s_c_t="-clang-analyzer-security.insecureAPI.DeprecatedOrUnsafeBufferHandling"
  s_c_t="$s_c_t,-clang-analyzer-valist.Uninitialized"
  s_c_t="$s_c_t,-clang-analyzer-unix.Malloc"
  s_c_t="$s_c_t,-clang-analyzer-security.insecureAPI.strcpy"
  s_c_t="$s_c_t,-clang-diagnostic-incompatible-pointer-types-discards-qualifiers"
  c_t="clang-tidy"
  q="--quiet"
  w="--warnings-as-errors"
  $c_t $q $w=* --checks=$s_c_t $(find . -name "*.[c|h]") -- $inc
fi

if command -v doxygen > /dev/null; then
  printf "rh build\n"
  clean
  export CC="gcc"
  ./rh_build.sh
  tests
fi

printf "gcc static build\n"
clean
export CC="gcc"
build --enable-static --disable-shared
tests

if [ "$os" != "freebsd" ] && ldconfig -p | grep -q libasan; then
  printf "\ngcc address sanitizer build\n"
  clean
  export CC="gcc"
  export CFLAGS="-fsanitize=address -O0 -ggdb"
  export LDFLAGS="-fsanitize=address -O0 -ggdb"
  build
  tests
fi

if command -v arm-linux-gnueabihf-gcc > /dev/null; then
  printf "\ngcc arm32 build\n"
  clean
  export CC="arm-linux-gnueabihf-gcc"
  build --host=arm-linux-gnueabihf
  if [ "$uname_m" == "aarch64" ]; then
    tests
  fi
fi

printf "\nclang build\n"
clean
export CC="clang"
build
tests

if command -v scan-build > /dev/null; then
  printf "\ngcc scan-build\n"
  clean

  export CC="gcc"
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

  export CC="clang"
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
export CC="clang"
build --enable-static --disable-shared
tests

if command -v cppcheck > /dev/null; then
  u="-U restrict -U __REDIRECT -U __restrict_arr -U __restrict"
  u="$u -U __REDIRECT_NTH -U _BSD_RUNE_T_ -U _TYPE_size_t -U __LDBL_REDIR1_DECL"
  supp="--suppress=missingInclude --suppress=unusedFunction"
  arg="-q --force $u --enable=all $inc $supp --error-exitcode=1"
  cppcheck="xargs cppcheck $arg"
  suppf="redblack.c"
  if find . -name "*.[c|h]" | grep -v "$suppf" | $cppcheck 2>&1 | grep ^; then
    false
  fi
fi

printf "\ngcc coverage build\n"
clean
export CC="gcc"
export CFLAGS="--coverage"
export LDFLAGS="--coverage"
build --enable-static --disable-shared
tests

bash <(curl -s https://codecov.io/bash)

if [ "$os" != "freebsd" ] && [ "$(uname -m)" == "x86_64" ]; then
  printf "\ncov-build build\n"
  clean

  file="/tmp/cov-analysis-${os}64.tar.gz"
  project="inotifytools"
  token="Dy7fkaSpHHjTg8JMFHKgOw"
  curl -o "$file" "https://scan.coverity.com/download/${os}64" \
    --form project="$project" --form token="$token"
  tar xf "$file"
  export CC="gcc"
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

