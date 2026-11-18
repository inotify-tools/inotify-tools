#!/bin/sh

test_description="Issue #195
When --fromfile is used with a list of a large number of files, it shouldn't crash
"

. ./sharness.sh

logfile="log"

run_() {
    # Setup a temp directory with a lot of temp files.
    tmp_dir=$(mktemp -d inotifywait-test-fromfile-XXXXXXXXX)
    for i in $(seq 2600); do
        mktemp -p "${tmp_dir}" test-file-XXX > /dev/null
    done
    logfile="$(realpath $logfile)"
    timeout=2 &&
    touch $logfile "${tmp_dir}"/test-file &&
    {(sleep 1 && chmod 777 "${tmp_dir}"/test-file)&}

    { cd "${tmp_dir}"; ls -1 | xargs realpath | $* \
        --quiet \
        --daemon \
        --outfile $logfile \
        --event ATTRIB \
        --fromfile - && sleep $timeout; }
}

run_and_check_log()
{
    rm -f $logfile
    run_ $* && grep "ATTRIB \$" $logfile
}

test_expect_success 'exit success' '
    run_and_check_log inotifywait
'

test_done
