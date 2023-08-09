#!/bin/bash

test_description='Check producing NUL-delimited output'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"

run_() {
  touch $logfile

  export LD_LIBRARY_PATH="../../libinotifytools/src/"

  ../../src/$* \
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
  sleep 2

  touch test-file-src

  mv test-file-src test-file-dst

  sleep 2

  kill ${PID}
}

run_and_check_log() {
	set -e
	trap "set +e" RETURN
	rm -f "${logfile}"
	run_ $*
	srcfile="${PWD}/test-file-src"
	dstfile="${PWD}/test-file-dst"

	return $(printf "${srcfile}\0${srcfile}\0${dstfile}\0" | cmp -s "${logfile}")
}

test_expect_success 'the output is delimited by NUL' '
    run_and_check_log inotifywait
'

if fanotify_supported; then
    test_expect_success 'the output is delimited by NUL' '
	run_and_check_log fsnotifywait --fanotify
    '
fi

test_done
