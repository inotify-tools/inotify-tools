#!/bin/sh

cp README.md README
autoreconf --install "$@" || exit 1
