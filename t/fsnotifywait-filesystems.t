#!/bin/sh

test_description='Filesystem-specific watch tests

Verify that inotifywait --fanotify works correctly
with various filesystem types
'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"

# Check if we can run filesystem tests
if ! is_root; then
    skip_all="filesystem tests require root privileges"
    test_done
fi

watch_create() {
    dir=$1
    file=$2
    watch=$3
    shift 3
    mkdir -p $dir &&
        {(sleep 1 && touch $dir/$file)&} &&
        ../../src/inotifywait \
            "$@" \
            --quiet \
            --outfile $logfile \
            --event CREATE \
            --timeout 3 \
            $watch
}

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    mnt=$1
    rm -rf $mnt/A &&
        watch_create $mnt/A/B/C/D test $mnt --filesystem
}

run_and_check_log()
{
    rm -f $logfile
    run_ $1 && grep 'CREATE.*test$' $logfile
}

# Test ext4 filesystem
if is_root && mount_filesystem_for_fanotify ext4 50M ext4_root; then
    test_expect_success 'filesystem watch works with ext4' '
        test_when_finished "cleanup_mounts ext4_root" &&
        run_and_check_log ext4_root
    '
fi

# Test tmpfs
if is_root && mount_tmpfs_for_fanotify tmpfs_root 10M; then
    test_expect_success 'filesystem watch works with tmpfs' '
        test_when_finished "cleanup_mounts tmpfs_root" &&
        run_and_check_log tmpfs_root
    '
fi

test_done
