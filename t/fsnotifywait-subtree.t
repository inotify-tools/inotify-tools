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
if fanotify_supported --filesystem && is_root; then
    test_expect_success 'event logged' '
        test_when_finished "umount -l root" &&
        mount_filesystem ext2 10M root &&
        run_and_check_log fsnotifywait --filesystem
    '
fi

# Report renamed and deleted directory paths during rm -rf without errors
if is_root && mount_tmpfs_for_fanotify root; then
    # A non-directory watch must resolve after a later directory event
    test_expect_success 'fsnotifywatch resolves a file watch after a directory event' '
        touch root/f &&
        export LD_LIBRARY_PATH="../../libinotifytools/src/" &&
        {(sleep 1 && mv root/f root/g && mkdir root/dd)&} &&
        ../../src/fsnotifywatch \
            --filesystem \
            --event MOVE_SELF \
            --event CREATE \
            --timeout 3 \
            root >stats.log 2>error.log &&
        grep "root/g$" stats.log &&
        grep "root/dd$" stats.log &&
        ! grep "Failed" error.log
    '

    test_expect_success 'rm -rf event keeps the directory path' '
        test_when_finished "cleanup_mounts root" &&
        rm -f $logfile &&
        export LD_LIBRARY_PATH="../../libinotifytools/src/" &&
        { ../../src/fsnotifywait \
            --filesystem \
            --monitor \
            --quiet \
            --outfile $logfile \
            --event CREATE \
            --event DELETE \
            --event DELETE_SELF \
            --event MOVE_SELF \
            --timeout 2 \
            root 2>error.log & } &&
        pid=$! &&
        sleep 1 &&
        mkdir -p root/d/x &&
        touch root/d/x/y &&
        # Rename while the watcher is running, so MOVE_SELF resolves the path.
        # The stopped rm -rf below is read after the directories are gone.
        mv root/d root/e &&
        mv root/e/x/y root/e/x/z &&
        sleep 1 &&
        # Stop the watcher so the directories are gone before it reads the events
        kill -STOP $pid &&
        rm -rf root/e &&
        kill -CONT $pid &&
        { wait $pid || test $? -eq 2; } &&
        grep "root/e/ MOVE_SELF,ISDIR $" $logfile &&
        grep "e/x/ MOVE_SELF z$" $logfile &&
        grep "root/./ DELETE,ISDIR x (deleted)$" $logfile &&
        grep "root/e/ DELETE_SELF,ISDIR (deleted)$" $logfile &&
        ! grep "^ " $logfile &&
        test_must_be_empty error.log
    '
fi

test_done
