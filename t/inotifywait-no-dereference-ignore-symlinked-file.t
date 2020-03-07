#!/bin/sh

test_description='--no-dereference causes inotifywait to ignore events on symlink target'

. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/.libs/"
    touch test &&
	ln -s test test-symlink &&
	{(sleep 1 && touch test)&} &&
    ../../src/.libs/inotifywait --quiet --no-dereference --timeout 2 test-symlink
}

test_expect_success 'Exit code 2 is returned' '
    test_expect_code 2 run_
'

test_done
