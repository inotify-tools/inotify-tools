#!/bin/sh

test_description='--no-dereference causes inotifywait to respond to events on symlink'

. ./fanotify-common.sh
. ./sharness.sh

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    rm -f test-symlink
    touch test &&
	ln -s test test-symlink &&
	{(sleep 1 && touch -h test-symlink)&} &&
    ../../src/$* --quiet --no-dereference --timeout 2 test-symlink
}

test_expect_success 'Exit code 0 is returned' '
    run_ inotifywait
'

# FIXME: Why is the root required?
if fanotify_supported && [ $(id -u) -eq 0 ]; then
    test_expect_success 'Exit code 0 is returned' '
        run_ fsnotifywait --fanotify
    '
fi

test_done
