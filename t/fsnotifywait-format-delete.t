#!/bin/sh

test_description='Issue #227: preserve file names in delete and move events'

. ./fanotify-common.sh
. ./sharness.sh

fanotify_supported && test_set_prereq FANOTIFY

# Queue the event while the watcher is stopped, so path resolution happens
# after the file has disappeared. This also covers a CREATE read after DELETE.
run_event() {
    mode=$1
    name=$2
    kind=$3
    event=$4
    action=$5
    shift 5

    rm -rf watched && mkdir watched &&
    watchpath="$(realpath watched)" &&
    path="$watchpath/$name" || return 1
    case "$kind" in
        file) touch "$path" ;;
        directory) mkdir "$path" ;;
        symlink) ln -s missing-target "$path" ;;
        new) : ;;
    esac || return 1

    ../../src/fsnotifywait "$mode" \
        --quiet --timeout 3 --event "$event" "$@" "$watchpath" \
        >actual 2>error &
    pid=$!
    test_when_finished '
        if test -n "$pid"; then
            kill -CONT "$pid" 2>/dev/null
            kill "$pid" 2>/dev/null
            wait "$pid" 2>/dev/null
        fi
        :
    ' &&
    sleep 1 &&
    kill -STOP "$pid" || return 1

    case "$action" in
        delete) rm -rf "$path" ;;
        move) mv "$path" "$watchpath/renamed" ;;
        create) touch "$path" ;;
        create-delete) touch "$path" && rm "$path" ;;
    esac &&
    kill -CONT "$pid" || return 1
    wait "$pid"
    status=$?
    pid=
    test "$status" -eq 0 &&
    test_must_be_empty error
}

for mode in -I -F; do
    prereq=
    test "$mode" != -F || prereq=FANOTIFY

    test_expect_success "$prereq" "$mode: deleted file keeps its name" '
        run_event "$mode" test file delete delete --format "%f" &&
        printf "test\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: deleted directory keeps its name and event type" '
        run_event "$mode" test directory delete delete --format "%f|%e" &&
        printf "test|DELETE,ISDIR\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: deleted symlink keeps its name" '
        run_event "$mode" test symlink delete delete --format "%f" &&
        printf "test\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: moved file keeps its old name and event type" '
        run_event "$mode" test file moved_from move --format "%f|%e" &&
        printf "test|MOVED_FROM\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: literal (deleted) suffix survives creation" '
        run_event "$mode" "test (deleted)" new create create --format "%f" &&
        printf "test (deleted)\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: literal (deleted) suffix survives deletion" '
        run_event "$mode" "test (deleted)" file delete delete --format "%f" &&
        printf "test (deleted)\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: queued creation keeps the original name after deletion" '
        run_event "$mode" test new create create-delete --format "%f" &&
        printf "test\n" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: full path formatting preserves deleted names" '
        run_event "$mode" test file delete delete --format "%w%f|%e" &&
        printf "%s/test|DELETE\n" "$watchpath" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: CSV preserves deleted names" '
        run_event "$mode" test file delete delete --csv &&
        printf "%s/,DELETE,test\n" "$watchpath" >expect &&
        test_cmp expect actual
    '

    test_expect_success "$prereq" "$mode: include filter matches the original deleted name" '
        run_event "$mode" test file delete delete --format "%f" --include "/test$" &&
        printf "test\n" >expect &&
        test_cmp expect actual
    '
done

test_done
