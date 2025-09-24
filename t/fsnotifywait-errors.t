#!/bin/sh

test_description='Error handling and edge cases

Verify that inotifywait handles error conditions gracefully
for --mount and --filesystem options
'

. ./fanotify-common.sh
. ./sharness.sh

# Test privilege requirements
if ! is_root; then
    test_expect_success 'filesystem watch requires appropriate privileges' '
        ../../src/inotifywait --filesystem --timeout 1 / 2>error.log
        status=$?
        test $status -ne 0 &&
        grep -q "Operation not permitted" error.log
    '

    test_expect_success 'mount watch requires appropriate privileges' '
        ../../src/inotifywait --mount --timeout 1 / 2>error.log
        status=$?
        test $status -ne 0 &&
        grep -q "Operation not permitted" error.log
    '
fi

# Test invalid paths
test_expect_success 'filesystem watch handles non-existent paths' '
    ! ../../src/inotifywait \
        --filesystem \
        --timeout 1 \
        /definitely/does/not/exist 2>error.log
'

test_expect_success 'mount watch handles non-existent paths' '
    ! ../../src/inotifywait \
        --mount \
        --timeout 1 \
        /definitely/does/not/exist 2>error.log
'

# Test that --mount and --filesystem automatically enable fanotify
test_expect_success 'mount option automatically enables fanotify' '
    ../../src/inotifywait \
        --mount \
        --timeout 1 \
        . 2>error.log || true &&
    ! grep -i "inotify" error.log
'

test_expect_success 'filesystem option automatically enables fanotify' '
    ../../src/inotifywait \
        --filesystem \
        --timeout 1 \
        . 2>error.log || true &&
    ! grep -i "inotify" error.log
'

# Test incompatible option combinations
test_expect_success 'mount and filesystem options are mutually exclusive' '
    ! ../../src/inotifywait \
        --mount \
        --filesystem \
        --timeout 1 \
        . 2>error.log
'

test_expect_success 'inotify and filesystem options are mutually exclusive' '
    ! ../../src/inotifywait \
        --inotify \
        --filesystem \
        --timeout 1 \
        . 2>error.log
'

test_expect_success 'inotify and mount options are mutually exclusive' '
    ! ../../src/inotifywait \
        --inotify \
        --mount \
        --timeout 1 \
        . 2>error.log
'

test_done
