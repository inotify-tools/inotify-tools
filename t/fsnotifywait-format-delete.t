#!/bin/sh

test_description='Issue #227: preserve file names in delete and move events'

. ./fanotify-common.sh
. ./sharness.sh

fanotify_supported && test_set_prereq FANOTIFY

# Filesystem and mount marks cover the whole filesystem/mount, so isolate them
# on tmpfs rather than watching the host filesystem containing the test tree.
if is_root && mount_tmpfs_for_fanotify event-root; then
    test_set_prereq FILESYSTEM
    fanotify_supported_on event-root --mount && test_set_prereq MOUNT
    cleanup_mounts event-root
fi

for mode in -I -F --filesystem; do
    case "$mode" in
        -I) prereq= ;;
        -F) prereq=FANOTIFY ;;
        --filesystem) prereq=FILESYSTEM ;;
    esac

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

# Mount marks cannot report DELETE or MOVED_FROM. Queue CLOSE_WRITE followed by
# unlink instead, so the same resolver sees a name whose file is already gone.
for name in test "test (deleted)"; do
    test_expect_success MOUNT "--mount: close before deletion preserves $name" '
        run_event --mount "$name" file close_write close-delete --format "%f|%e" &&
        printf "%s|CLOSE_WRITE,CLOSE\n" "$name" >expect &&
        test_cmp expect actual
    '
done

test_expect_success MOUNT "--mount: full path preserves a deleted name" '
    run_event --mount test file close_write close-delete --format "%w%f|%e" &&
    printf "%s/test|CLOSE_WRITE,CLOSE\n" "$watchpath" >expect &&
    test_cmp expect actual
'

test_expect_success MOUNT "--mount: CSV preserves a deleted name" '
    run_event --mount test file close_write close-delete --csv &&
    printf "%s/,\"CLOSE_WRITE,CLOSE\",test\n" "$watchpath" >expect &&
    test_cmp expect actual
'

test_expect_success MOUNT "--mount: include filter matches the original name" '
    run_event --mount test file close_write close-delete --format "%f" --include "/test$" &&
    printf "test\n" >expect &&
    test_cmp expect actual
'

test_done
