#!/bin/sh

test_description='Recursive watch with include filter

Verify that:
1. Only files matching include pattern are shown in events
2. Directory events are not shown but directories are still watched
3. Files matching pattern in subdirectories trigger events
'

. ./sharness.sh

logfile="log"

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    testdir=root/A/B
    
    rm -rf root && mkdir -p $testdir || return 1

    # Start inotifywait in monitor mode
    ../../src/inotifywait \
        --quiet \
        --monitor \
        --recursive \
        --outfile $logfile \
        --include "\.txt$" \
        --event CREATE \
        --format "%w%f" \
        root &
    
    inotifywait_pid=$!

    # Wait for watches to be established
    sleep 1

    # Create test files
    touch $testdir/test.dat
    touch $testdir/test.txt
    mkdir $testdir/subdir
    touch $testdir/subdir/nested.txt
    touch $testdir/subdir/nested.dat

    # Give inotifywait time to process events
    sleep 1

    # Kill inotifywait
    kill $inotifywait_pid
    wait $inotifywait_pid || true
}

test_expect_success 'correct events logged for recursive watch with include filter' '
    rm -f $logfile &&
    run_ &&
    test -f $logfile &&
    test $(grep -c "test.txt\|nested.txt" $logfile) = 2 &&
    grep "root/A/B/test.txt" $logfile &&
    grep "root/A/B/subdir/nested.txt" $logfile &&
    ! grep "test.dat" $logfile &&
    ! grep "nested.dat" $logfile &&
    ! grep "subdir$" $logfile
'

test_done