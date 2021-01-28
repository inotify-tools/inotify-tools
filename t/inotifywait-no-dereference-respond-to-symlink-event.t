#!/bin/sh

test_description='--no-dereference causes inotifywait to respond to events on symlink'

. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    touch test &&
	ln -s test test-symlink &&
	{(sleep 1 && touch -h test-symlink)&} &&
    ../../src/inotifywait --quiet --no-dereference --timeout 2 test-symlink
}

test_expect_success 'Exit code 0 is returned' '
    run_
'

test_done
