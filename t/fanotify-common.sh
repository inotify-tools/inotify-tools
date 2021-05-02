#!/bin/sh

# Check for kernel support and privileges
fanotify_supported() {
    ../../src/fsnotifywait --fanotify 2>&1 | grep -q 'No files specified'
}
