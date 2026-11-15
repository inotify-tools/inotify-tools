#!/bin/sh

test_description='Filtered events respect the inotifywait timeout'

. ./sharness.sh

run_filtered_wait() {
    accept=$1
    rm -rf watched actual error noise-started noise-finished &&
    mkdir watched && : >error || return 1

    ../../src/inotifywait --timeout 1 --event close_write \
        --exclude '/noise$' --format '%f' watched >actual 2>error &
    watcher=$!
    noise_pid=
    test_when_finished '
        for pid in "$watcher" "$noise_pid"; do
            if test -n "$pid"; then
                kill "$pid" 2>/dev/null
                wait "$pid" 2>/dev/null
            fi
        done
        :
    ' || return 1

    # Confirm watch setup before generating either ignored or accepted events.
    attempts=0
    while ! grep -q 'Watches established' error; do
        kill -0 "$watcher" 2>/dev/null || return 1
        attempts=$((attempts + 1))
        test "$attempts" -lt 100 || return 1
        sleep 0.01
    done

    (
        count=0
        while test "$count" -lt 100; do
            printf noise >watched/noise || exit 1
            : >noise-started
            count=$((count + 1))
            sleep 0.05
        done
        touch noise-finished
    ) &
    noise_pid=$!

    attempts=0
    while test ! -e noise-started; do
        kill -0 "$noise_pid" 2>/dev/null || return 1
        attempts=$((attempts + 1))
        test "$attempts" -lt 100 || return 1
        sleep 0.01
    done

    if test "$accept" = yes; then
        printf target >watched/target || return 1
    fi

    wait "$watcher"
    status=$?
    watcher=
    if test "$accept" = yes; then
        test "$status" -eq 0 &&
        printf 'target\n' >expect &&
        test_cmp expect actual
    else
        test "$status" -eq 2 &&
        test_must_be_empty actual &&
        test ! -e noise-finished &&
        kill -0 "$noise_pid"
    fi
}

test_expect_success 'excluded noise cannot keep a timed wait alive' '
    run_filtered_wait no
'

test_expect_success 'a matching event is returned while noise is excluded' '
    run_filtered_wait yes
'

test_done
