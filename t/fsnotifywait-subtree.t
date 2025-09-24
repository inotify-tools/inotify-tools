#!/bin/sh

test_description='Subtree watch

Verify that inotifywait --recursive/--filesystem gets events on
files created inside the watched subtree
'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"
subdir="root/A"
extdir="root/X"

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    testdir=root/A/B/C/D
        mkdir -p $testdir &&
	{(sleep 1 && touch $extdir/ignore $testdir/test 2>/dev/null)&} &&
    ../../src/$* \
        --quiet \
        --outfile $logfile \
        --event CREATE \
        --timeout 2 \
        $subdir
}

run_and_check_log()
{
    rm -f $logfile
    run_ $* && grep 'CREATE.test$' $logfile
}

test_expect_success 'event logged' '
    rm -rf root &&
    run_and_check_log inotifywait --recursive
'

if fanotify_supported; then
    test_expect_success 'event logged' '
        rm -rf root &&
        run_and_check_log inotifywait --fanotify --recursive
    '
fi

# root requirement:
# https://github.com/inotify-tools/inotify-tools/pull/183#issuecomment-1635518850
if fanotify_supported --filesystem && is_root; then
    test_expect_success 'event logged' '
        test_when_finished "umount -l root" &&
        mount_filesystem ext2 10M root &&
        run_and_check_log inotifywait --filesystem
    '
fi

# Create files outside bind mount ($extdir) and inside bind mount ($subdir)
# Expect to see only the log about the file created inside bind mount
if fanotify_supported --filesystem && is_root; then
    test_expect_success 'event logged' '
        test_when_finished "umount -l $subdir root" &&
        mount_filesystem ext2 10M root &&
        mkdir -p $subdir $extdir &&
        mount --bind $subdir $subdir &&
        run_and_check_log inotifywait --filesystem
    '
fi

test_done
