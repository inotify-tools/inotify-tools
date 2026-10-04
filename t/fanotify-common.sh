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

# Clean up filesystem mounts
cleanup_mounts() {
    for mnt in "$@"; do
        if mountpoint -q "$mnt" 2>/dev/null; then
            umount -l "$mnt" 2>/dev/null || true
        fi
        rm -rf "$mnt" 2>/dev/null || true
    done
}
