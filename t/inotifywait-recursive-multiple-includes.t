#!/bin/sh

test_description='Recursive watch with multiple include filters

Verify that:
1. Files matching any include pattern trigger events
2. Files not matching any pattern are ignored
3. Directory events are not shown but directories are still watched
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
        --include ".*\.(txt|log)$" \
        --event CREATE \
        --format "%w%f" \
        root &
    
    inotifywait_pid=$!

    # Wait for watches to be established
    sleep 1

    # Create test files
    touch $testdir/test.dat
    touch $testdir/test.txt
    touch $testdir/info.log
    mkdir $testdir/subdir
    touch $testdir/subdir/nested.txt
    touch $testdir/subdir/data.log
    touch $testdir/subdir/other.dat

    # Give inotifywait time to process events
    sleep 1

    # Kill inotifywait
    kill $inotifywait_pid
    wait $inotifywait_pid || true
}

test_expect_success 'correct events logged for multiple include filters' '
    rm -f $logfile &&
    run_ &&
    test -f $logfile &&
    test $(grep -c "txt\|log" $logfile) = 4 &&
    grep "root/A/B/test.txt" $logfile &&
    grep "root/A/B/info.log" $logfile &&
    grep "root/A/B/subdir/nested.txt" $logfile &&
    grep "root/A/B/subdir/data.log" $logfile &&
    ! grep "test.dat" $logfile &&
    ! grep "other.dat" $logfile &&
    ! grep "subdir$" $logfile
'

test_done 