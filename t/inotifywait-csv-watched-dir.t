#!/bin/sh

test_description='Issue #157

Check --csv format of event on a file when watching the parent directory
'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"
watchpath="$(realpath .)"

run_() {
    # Setup code, defer an ATTRIB event for after
    # inotifywait has been set up.
    timeout=4 &&
    touch $logfile test-file &&
    {(sleep 2 && chmod 777 test-file)&} &&

    export LD_LIBRARY_PATH="../../libinotifytools/src/"
    ../../src/$* \
        --csv \
        --quiet \
        --daemon \
        --outfile $logfile \
        --event ATTRIB \
        --timeout $timeout \
        "$watchpath" &&
    # No way to use 'wait' for a process that is not a child of this one,
    # sleep instead until inotifywait's timeout is reached.
    sleep $timeout
}

run_and_check_log()
{
    rm -f $logfile
    run_ $* && cp log /tmp/ && grep "^$watchpath/,ATTRIB,test-file\$" $logfile
}

test_expect_success 'event logged' '
    run_and_check_log inotifywait
'

if fanotify_supported; then
    test_expect_success 'event logged' '
	run_and_check_log fsnotifywait --fanotify
    '
fi

test_done
