#!/bin/sh

test_description='No event occured for inotifywait'

. ./sharness.sh

run_() {
    touch test &&
    inotifywait --quiet --timeout 1 test
}

test_expect_success 'Exit code 2 is returned' '
    test_expect_code 2 run_
'

test_done
