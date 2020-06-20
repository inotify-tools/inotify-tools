#!/bin/sh

test_description='Issue #62

When --daemon is used, events are logged correctly to --outfile
even if that is a relative path
'

. ./sharness.sh

logfile="log"

run_() {
    # Setup code, defer an ATTRIB event for after
    # inotifywait has been set up.
    timeout=2 &&
    touch $logfile test-file &&
    {(sleep 1 && chown $(id -u) test-file)&} &&

    export LD_LIBRARY_PATH="../../libinotifytools/src/.libs/"
    ../../src/.libs/inotifywait \
        --quiet \
        --daemon \
        --outfile $logfile \
        --event ATTRIB \
        --timeout $timeout \
        $(realpath test-file) &&
    # No way to use 'wait' for a process that is not a child of this one,
    # sleep instead until inotifywait's timeout is reached.
    sleep $timeout
}

test_expect_success 'event logged' '
    run_ &&
    grep ATTRIB $logfile
'

test_done
