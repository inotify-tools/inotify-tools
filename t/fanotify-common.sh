#!/bin/sh

# Check for kernel support and privileges on a path.
# Extra arguments are fsnotifywait options, such as --filesystem.
fanotify_supported_on() {
    path=$1
    shift
    ../../src/fsnotifywait --fanotify -t -1 "$@" "$path" 2>&1 | grep -q 'Negative timeout'
}

fanotify_supported() {
    fanotify_supported_on "." "$@"
}

# Create and mount a test filesystem
mount_filesystem() {
    fstype=$1
    size=$2
    mnt=$3
    rm -f img
    truncate -s $size img && mkfs.$fstype img && \
        mkdir -p $mnt && mount -o loop img $mnt && \
        df -t $fstype $mnt
}

# Mount `fstype` of `size` at `mnt` when a filesystem watch can run there.
# Returns 0 with `mnt` still mounted. Otherwise prints a SKIP line and
# returns 1.
mount_filesystem_for_fanotify() {
    fstype=$1
    size=$2
    mnt=$3
    if mount_filesystem $fstype $size $mnt &&
        fanotify_supported_on $mnt --filesystem
    then
        return 0
    fi
    cleanup_mounts $mnt
    echo "# SKIP: filesystem watch not supported on $fstype"
    return 1
}

# Create tmpfs mount
mount_tmpfs() {
    mnt=$1
    size=${2:-10M}
    mkdir -p $mnt && mount -t tmpfs -o size=$size tmpfs $mnt
}

# Mount tmpfs of `size` at `mnt` when a filesystem watch can run there.
# Returns 0 with `mnt` still mounted. Otherwise prints a SKIP line and
# returns 1.
mount_tmpfs_for_fanotify() {
    mnt=$1
    size=${2:-10M}
    if mount_tmpfs $mnt $size &&
        fanotify_supported_on $mnt --filesystem
    then
        return 0
    fi
    cleanup_mounts $mnt
    echo "# SKIP: filesystem watch not supported on tmpfs"
    return 1
}

# Create and mount a btrfs filesystem with subvolumes
mount_btrfs_with_subvolumes() {
    size=$1
    mnt=$2
    subvol_name=${3:-subvol1}

    mount_filesystem btrfs $size $mnt && \
        btrfs subvolume create $mnt/$subvol_name
}

# Create an overlayfs mount
mount_overlayfs() {
    base_dir=$1
    work_dir=${base_dir}_work
    upper_dir=${base_dir}_upper
    overlay_dir=${base_dir}_overlay

    mkdir -p $base_dir $work_dir $upper_dir $overlay_dir && \
        mount -t overlay overlay \
        -o lowerdir=$base_dir,upperdir=$upper_dir,workdir=$work_dir \
        $overlay_dir
}

# Test if we're running as root
is_root() {
    [ $(id -u) -eq 0 ]
}

# Test if we can create and mount a btrfs filesystem
btrfs_supported() {
    which mkfs.btrfs >/dev/null 2>&1 && which btrfs >/dev/null 2>&1 || return 1
    img=$(mktemp) || return 1
    mnt=$(mktemp -d) || { rm -f "$img"; return 1; }
    truncate -s 120M "$img" && \
        mkfs.btrfs "$img" >/dev/null 2>&1 && \
        mount -o loop "$img" "$mnt" >/dev/null 2>&1
    rc=$?
    umount "$mnt" >/dev/null 2>&1
    rm -rf "$mnt" "$img"
    return $rc
}

# Test if overlayfs is supported
overlayfs_supported() {
    grep -q overlay /proc/filesystems 2>/dev/null
}

# Run a binary chrooted to $1 in a private mount namespace
run_in_chroot() {
    root=$1
    shift
    unshare -m sh -c '
        root=$1
        bind_file() {
            mkdir -p "$root$(dirname "$1")" &&
                touch "$root$1" &&
                mount --bind "$1" "$root$1"
        }
        mount --make-rprivate / &&
            mkdir -p "$root/proc" &&
            mount -t proc proc "$root/proc" &&
            bind_file "$2" &&
            for lib in $(ldd "$2" | grep -o "/[^ ]*"); do
                bind_file "$lib" || exit 1
            done &&
            shift &&
            exec chroot "$root" "$@"
    ' sh "$root" "$@"
}

# Clean up filesystem mounts
cleanup_mounts() {
    for mnt in "$@"; do
        if mountpoint -q "$mnt" 2>/dev/null; then
            umount -l "$mnt" 2>/dev/null || true
        fi
        rm -rf "$mnt" 2>/dev/null || true
    done
}

# Single-event runner for deletion-regression tests.
#
# run_event MODE NAME KIND EVENT ACTION [fsnotifywait options...]
# KIND: file, directory, symlink (dangling), or new (not created yet).
# ACTION: delete, move, create, create-delete, or close-delete.
# Writes stdout/stderr to actual/error and requires a successful exit and empty
# stderr. Sets watchpath to the absolute parent directory and path to the child.
# Filesystem and mount modes use a private tmpfs mount, cleaned up after each
# test even if the action or an assertion fails. Other modes use watched/.
#
# Pause the watcher before the action so queued events are processed after the
# action finishes. In particular, close-delete closes the file before unlinking
# it, letting mount watches exercise missing names with a supported event.
run_event() {
    event_mode=$1
    event_name=$2
    event_kind=$3
    event_mask=$4
    event_action=$5
    shift 5

    event_dir=watched
    case "$event_mode" in
        --filesystem|--mount)
            test_when_finished 'cleanup_mounts event-root' &&
            mount_tmpfs event-root || return 1
            event_dir=event-root/watched
            ;;
    esac

    rm -rf "$event_dir" && mkdir "$event_dir" &&
    watchpath="$(realpath "$event_dir")" &&
    path="$watchpath/$event_name" || return 1
    case "$event_kind" in
        file) touch "$path" ;;
        directory) mkdir "$path" ;;
        symlink) ln -s missing-target "$path" ;;
        new) : ;;
        *) return 1 ;;
    esac || return 1

    ../../src/fsnotifywait "$event_mode" \
        --quiet --timeout 5 --event "$event_mask" "$@" "$watchpath" \
        >actual 2>error &
    event_pid=$!
    test_when_finished '
        if test -n "$event_pid"; then
            kill -CONT "$event_pid" 2>/dev/null
            kill "$event_pid" 2>/dev/null
            wait "$event_pid" 2>/dev/null
        fi
        :
    ' &&
    sleep 1 &&
    kill -STOP "$event_pid" || return 1

    case "$event_action" in
        delete) rm -rf "$path" ;;
        move) mv "$path" "$watchpath/renamed" ;;
        create) touch "$path" ;;
        create-delete) touch "$path" && rm "$path" ;;
        close-delete) printf "data\n" >"$path" && rm "$path" ;;
        *) return 1 ;;
    esac &&
    kill -CONT "$event_pid" || return 1
    wait "$event_pid"
    event_status=$?
    event_pid=
    test "$event_status" -eq 0 &&
    test_must_be_empty error
}
