#!/bin/sh

test_description='No event occured for inotifywait'

. ./fanotify-common.sh
. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    touch test &&
    ../../src/$* --quiet --timeout 1 test
}

test_expect_success 'Exit code 2 is returned' '
    test_expect_code 2 run_ inotifywait
'

if fanotify_supported; then
    test_expect_success 'Exit code 2 is returned' '
        test_expect_code 2 run_ fsnotifywait --fanotify
    '
fi

test_done
