#!/bin/bash

set -e

if [ -n "$1" ]; then
  j="$1"
else
  ./autogen.sh
  ./configure --disable-dependency-tracking --disable-static --enable-doxygen
  j="-j16"
fi

# https://docs.fedoraproject.org/en-US/packaging-guidelines/#_removing_rpath
sed -i 's|^hardcode_libdir_flag_spec=.*|hardcode_libdir_flag_spec=""|g' libtool
sed -i 's|^runpath_var=LD_RUN_PATH|runpath_var=DIE_RPATH_DIE|g' libtool
make $j

