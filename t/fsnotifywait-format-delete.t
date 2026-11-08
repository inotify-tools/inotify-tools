#!/bin/sh

test_description='Issue #227: preserve file names in delete and move events'

. ./fanotify-common.sh
. ./sharness.sh

fanotify_supported && test_set_prereq FANOTIFY

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
