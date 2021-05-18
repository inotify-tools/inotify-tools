#!/bin/sh

# Check for kernel support and privileges
fanotify_supported() {
    ../../src/fsnotifywait --fanotify $* 2>&1 | grep -q 'No files specified'
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
