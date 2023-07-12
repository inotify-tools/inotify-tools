#!/bin/sh

# Check for kernel support and privileges
fanotify_supported() {
    if [ -z "$(grep 'kthreadd' /proc/2/status 2>/dev/null)" ]; then
        # FIXME: fanotify is broken in containers
        # https://stackoverflow.com/a/72136877/2995591
        # https://github.com/inotify-tools/inotify-tools/pull/183
        false
    else
        ../../src/fsnotifywait --fanotify $* 2>&1 | grep -q 'No files specified'
    fi
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
