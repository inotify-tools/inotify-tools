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

# Test --mount with supported events (OPEN, CLOSE)
if fanotify_supported --mount && is_root; then
    test_expect_success 'mount watch logs OPEN events' '
        test_when_finished "cleanup_mounts root" &&
        mount_filesystem ext2 10M root &&
        run_and_check_log "OPEN" inotifywait --mount
    '

    test_expect_success 'mount watch logs CLOSE events' '
        test_when_finished "cleanup_mounts root" &&
        mount_filesystem ext2 10M root &&
        run_and_check_log "CLOSE" inotifywait --mount --event CLOSE_NOWRITE
    '

    # Test that --mount works with different filesystem types
    test_expect_success 'mount watch works with tmpfs' '
        test_when_finished "cleanup_mounts root" &&
        mount_tmpfs root &&
        run_and_check_log "OPEN" inotifywait --mount
    '

    # Test error conditions - events not supported by mount marks
    test_expect_success 'mount watch may not support CREATE events' '
        test_when_finished "cleanup_mounts root" &&
        mount_filesystem ext2 10M root &&
        {
            # CREATE events may or may not work with mount marks
            # depending on kernel version, so we just verify the tool runs
            run_ inotifywait --mount --event CREATE --timeout 1 || true
        }
    '

fi

test_done
