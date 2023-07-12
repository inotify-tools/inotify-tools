bin_PROGRAMS = inotifywait inotifywatch
inotifywait_SOURCES = inotifywait.cpp common.cpp common.h
inotifywatch_SOURCES = inotifywatch.cpp common.cpp common.h

if ENABLE_FANOTIFY
# Build the fsnotify* tools with fanotify as the default backend
bin_PROGRAMS += fsnotifywait fsnotifywatch
fsnotifywait_SOURCES = inotifywait.cpp common.cpp common.h
fsnotifywait_CPPFLAGS = $(AM_CPPFLAGS) -DENABLE_FANOTIFY
fsnotifywatch_SOURCES = inotifywatch.cpp common.cpp common.h
fsnotifywatch_CPPFLAGS = $(AM_CPPFLAGS) -DENABLE_FANOTIFY
endif

AM_CFLAGS = -Wall -Wextra -Wshadow -Werror -std=c99 -I../libinotifytools/src
AM_CPPFLAGS = -I$(top_srcdir)/libinotifytools/src
LDADD = ../libinotifytools/src/libinotifytools.la

if STATIC_BINARY_ENABLE
AM_LDFLAGS = -static-libtool-libs
endif
