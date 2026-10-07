#!/bin/sh

test_description='Mount watch

Verify that inotifywait --mount gets events on
files within the mounted filesystem
'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    testdir=root/A/B/C/D
    rm -rf root/A &&
        mkdir -p $testdir &&
	{(sleep 1 && touch $testdir/test && cat $testdir/test)&} &&
    ../../src/$* \
        --quiet \
        --outfile $logfile \
        --timeout 3 \
        root
}

run_and_check_log()
{
    rm -f $logfile
    pattern="$1"
    shift
    run_ $* && grep "$pattern" $logfile
}

# Check if we can run mount tests
if ! is_root; then
    skip_all="mount tests require root privileges"
    test_done
fi

if mount_tmpfs_for_fanotify root; then
    test_expect_success 'mount watch logs OPEN events' '
        run_and_check_log "OPEN" inotifywait --mount
    '

    test_expect_success 'mount watch logs CLOSE events' '
        run_and_check_log "CLOSE" inotifywait --mount --event CLOSE_NOWRITE
    '

    # Test error conditions - events not supported by mount marks
    test_expect_success 'mount watch rejects CREATE events' '
        export LD_LIBRARY_PATH="../../libinotifytools/src/" &&
        ! ../../src/inotifywait \
            --mount \
            --event CREATE \
            --timeout 1 \
            root 2>error.log &&
        grep -q "add mount watch root: Invalid argument" error.log
    '

    cleanup_mounts root
fi

test_done
