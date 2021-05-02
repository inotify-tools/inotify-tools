#!/bin/sh

test_description='--no-dereference causes inotifywait to ignore events on symlink target'

. ./fanotify-common.sh
. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    rm -f test-symlink
    touch test &&
	ln -s test test-symlink &&
	{(sleep 1 && touch test)&} &&
    ../../src/$* --quiet --no-dereference --timeout 2 test-symlink
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
