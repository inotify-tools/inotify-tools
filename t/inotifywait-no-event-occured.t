#!/bin/sh

test_description='No event occured for inotifywait'

. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    touch test &&
    ../../src/inotifywait --quiet --timeout 1 test
}

test_expect_success 'Exit code 2 is returned' '
    test_expect_code 2 run_
'

test_done
