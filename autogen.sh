#!/bin/sh

aclocal && autoconf && autoheader && libtoolize && automake --add-missing && echo "OK, you can run \`./configure' now."

