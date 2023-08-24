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

if IS_CLANG
AM_CFLAGS = -std=c99 -fno-exceptions -Wall -Wextra -Wshadow -Werror -Wno-unused-command-line-argument -I../libinotifytools/src -I$(srcdir)/../libinotifytools/src
AM_CPPFLAGS = -std=c++17 -fno-exceptions -Wall -Wextra -Wshadow -Werror -Wno-unused-command-line-argument -I../libinotifytools/src -I$(srcdir)/../libinotifytools/src
else
AM_CFLAGS = -std=c99 -fno-exceptions -Wall -Wextra -Wshadow -Werror -I../libinotifytools/src -I$(srcdir)/../libinotifytools/src
AM_CPPFLAGS = -std=c++17 -fno-exceptions -Wall -Wextra -Wshadow -Werror -I../libinotifytools/src -I$(srcdir)/../libinotifytools/src
endif

LDADD = ../libinotifytools/src/libinotifytools.la

if STATIC_BINARY_ENABLE
AM_LDFLAGS = -static-libtool-libs
endif
