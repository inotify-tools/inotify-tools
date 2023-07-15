#!/bin/sh

test_description='Subtree watch

Verify that fsnotifywait --recursive/--filesystem gets events on
files created inside the watched subtree
'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"

run_() {
    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    testdir=root/A/B/C/D
    rm -rf root/A &&
        mkdir -p $testdir &&
	{(sleep 1 && touch $testdir/test)&} &&
    ../../src/$* \
        --quiet \
        --outfile $logfile \
        --event CREATE \
        --timeout 2 \
        root
}

run_and_check_log()
{
    rm -f $logfile
    run_ $* && grep 'CREATE.test$' $logfile
}

test_expect_success 'event logged' '
    run_and_check_log inotifywait --recursive
'

if fanotify_supported; then
    test_expect_success 'event logged' '
        run_and_check_log fsnotifywait --fanotify --recursive
    '
fi

# root requirement:
# https://github.com/inotify-tools/inotify-tools/pull/183#issuecomment-1635518850
if fanotify_supported --filesystem && [ $(id -u) -eq 0 ]; then
    test_expect_success 'event logged' '
        test_when_finished "umount -l root" &&
        mount_filesystem ext2 10M root &&
        run_and_check_log fsnotifywait --filesystem
    '
fi

test_done
