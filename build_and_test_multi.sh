#!/bin/bash

set -e

if command -v podman > /dev/null; then
  container="podman"
elif command -v docker > /dev/null; then
  container="docker"
fi

container_run() {
  cmd="./build_and_test.sh $1"
  id=$(sudo $container run --privileged -d -it $1 /bin/sh)
  sudo $container exec -it $id /bin/sh -c "mkdir -p $PWD"
  sudo $container cp $PWD $id:$PWD/..
  sudo $container exec -it $id /bin/sh -c "cd $PWD && $cmd"
  sudo $container rm -f $id
}

if [ -n "$container" ]; then
#  container_run "alpine:3.17"
#  container_run "centos:stream9"
#  container_run "fedora:37"
  container_run "ubuntu:22.04"
  container_run "ubuntu:20.04"
  container_run "ubuntu:18.04"
  exit 0
fi

$cmd

