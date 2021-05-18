#!/bin/bash

test_description='Resolves Issue #72

Make transaction id (cookie) available as part of the format string using %c'

. ./fanotify-common.sh
. ./sharness.sh

logfile="log"

run_() {
  # Setup code, defer an ATTRIB event for after
  # inotifywait has been set up.
  touch $logfile

  export LD_LIBRARY_PATH="../../libinotifytools/src/"

  ../../src/$* \
    --timeout 4 \
    --monitor \
    --daemon \
    --quiet \
    --outfile $logfile \
    --format '%c %e %w%f' \
    --event create \
    --event moved_to \
    --event moved_from \
    $(realpath ./)

  sleep 1

  touch test-file-src

  mv test-file-src test-file-dst

  sleep 1
}

run_and_check_log()
{
	set -e
	trap "set +e" RETURN
	rm -f "${logfile}"
	run_ $*
	local NONCOOKIE="$(cat "${logfile}" | sed -n 1p | grep -Eo "^[^ ]+")"
	#Make sure cookie is 0 for single events
	[[ "${NONCOOKIE}" == "0" ]] || return 1
	local COOKIE_A="$(cat "${logfile}" | sed -n 2p | grep -Eo "^[^ ]+")"
	[[ -n "${COOKIE_A}" ]] || return 2
	local COOKIE_B="$(cat "${logfile}" | sed -n 3p | grep -Eo "^[^ ]+")"
	[[ "${COOKIE_A}" == "${COOKIE_B}" ]] || return 3
	cat "${logfile}"
}

test_expect_success 'event logged' '
    run_and_check_log inotifywait
'

if fanotify_supported; then
    test_expect_success 'event logged' '
	run_and_check_log fsnotifywait --fanotify
    '
fi

test_done

