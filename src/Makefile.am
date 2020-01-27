bin_PROGRAMS = inotifywait inotifywatch
inotifywait_SOURCES = inotifywait.c common.c common.h
inotifywatch_SOURCES = inotifywatch.c common.c common.h

AM_CFLAGS = -Wall -Werror -Wpointer-arith -std=c99 -I../libinotifytools/src
AM_CPPFLAGS = -I$(top_srcdir)/libinotifytools/src
LDADD = ../libinotifytools/src/libinotifytools.la

if STATIC_BINARY_ENABLE
AM_LDFLAGS = -static-libtool-libs
endif
