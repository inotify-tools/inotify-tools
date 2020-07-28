#!/bin/bash

test_description='Check producing NUL-delimited output'

. ./sharness.sh

logfile="log"

run_() {
  touch $logfile

  export LD_LIBRARY_PATH="../../libinotifytools/src/.libs/"

  ../../src/.libs/inotifywait \
    --monitor \
    --quiet \
    --outfile $logfile \
    --format '%w%f%0' \
    --no-newline \
    --event create \
    --event moved_to \
    --event moved_from \
    $(realpath ./) &

  PID="$!"
  sleep 1

  touch test-file-src

  mv test-file-src test-file-dst

  sleep 1

  kill ${PID}
}

test_expect_success 'the output is delimited by NUL' \
	'
	set -e
	trap "set +e" RETURN
	run_
	srcfile="${PWD}/test-file-src"
	dstfile="${PWD}/test-file-dst"

	return $(printf "${srcfile}\0${srcfile}\0${dstfile}\0" | cmp -s "${logfile}")
	'

test_done
