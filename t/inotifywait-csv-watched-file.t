#!/bin/sh

test_description='Issue #157

Check --csv format of event on a file when watching the file itself
'

. ./sharness.sh

logfile="log"
watchpath="$(realpath test-file)"

run_() {
    # Setup code, defer an ATTRIB event for after
    # inotifywait has been set up.
    timeout=2 &&
    touch $logfile test-file &&
    {(sleep 1 && chmod 777 test-file)&} &&

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
    run_ $* && grep "^$watchpath,ATTRIB,\$" $logfile
}

test_expect_success 'event logged' '
    run_and_check_log inotifywait
'

test_done
