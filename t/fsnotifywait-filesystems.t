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

# Test btrfs filesystem support
if is_root && btrfs_supported; then
if mount_filesystem_for_fanotify btrfs 120M btrfs_root; then
    test_expect_success 'filesystem watch works with btrfs' '
        test_when_finished "cleanup_mounts btrfs_root" &&
        run_and_check_log btrfs_root
    '

    test_expect_success 'filesystem watch detects changes in btrfs subvolumes' '
        test_when_finished "cleanup_mounts btrfs_root" &&
        mount_btrfs_with_subvolumes 120M btrfs_root subvol1 &&
        watch_create btrfs_root/subvol1/testdir subvol_file btrfs_root --filesystem &&
        grep -q "CREATE.*subvol_file" $logfile
    '

    # Check if btrfs subvolumes support fanotify directory watches (since v6.8)
    mount_btrfs_with_subvolumes 120M btrfs_check subvol1 >/dev/null || {
        cleanup_mounts btrfs_check
        test_skip_btrfs_fanotify="Btrfs subvolume mount failed"
    }

    if test -z "$test_skip_btrfs_fanotify"; then
        mkdir -p btrfs_check/subvol1/testdir &&
        ../../src/inotifywait --fanotify --timeout -1 btrfs_check/subvol1/testdir 2>error.log || {
            if grep -q "Invalid cross-device link" error.log; then
                test_skip_btrfs_fanotify="Btrfs subvolume fanotify watches not supported"
            fi
        }
        cleanup_mounts btrfs_check
    fi

    if test -z "$test_skip_btrfs_fanotify"; then
        test_expect_success 'fanotify directory watch works inside btrfs subvolumes' '
            test_when_finished "cleanup_mounts btrfs_root" &&
            mount_btrfs_with_subvolumes 120M btrfs_root subvol1 &&
            rm -f $logfile &&
            watch_create btrfs_root/subvol1/testdir subvol_file \
                btrfs_root/subvol1/testdir --fanotify &&
            grep -q "CREATE.*subvol_file" $logfile
        '
    else
        echo "# SKIP: $test_skip_btrfs_fanotify"
    fi
fi
else
    echo "# SKIP: btrfs not supported"
fi

# Test overlayfs
if fanotify_supported && overlayfs_supported; then
    # Check if overlayfs supports fanotify directory watches (since v6.6)
    mkdir -p overlay_test_check
    mount_overlayfs overlay_test_check 2>/dev/null || {
        cleanup_mounts overlay_test_check overlay_test_check_overlay
        test_skip_overlayfs="Overlayfs mount failed"
    }

    if test -z "$test_skip_overlayfs"; then
        ../../src/inotifywait --fanotify --timeout -1 overlay_test_check_overlay 2>error.log || {
            if grep -q "Operation not supported" error.log; then
                test_skip_overlayfs="Overlayfs does not support fanotify watches"
            fi
        }
        cleanup_mounts overlay_test_check overlay_test_check_overlay
    fi

    if test -z "$test_skip_overlayfs"; then
        test_expect_success 'fanotify directory watch works with overlayfs' '
            test_when_finished "cleanup_mounts overlay_test overlay_test_overlay" &&
            mkdir -p overlay_test &&
            mount_overlayfs overlay_test &&
            watch_create overlay_test_overlay/testdir overlay_file \
                overlay_test_overlay/testdir --fanotify &&
            grep -q "CREATE.*overlay_file" $logfile
        '
    else
        echo "# SKIP: $test_skip_overlayfs"
    fi
fi

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
