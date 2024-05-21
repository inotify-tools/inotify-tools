bin_PROGRAMS = inotifywait inotifywatch
inotifywait_SOURCES = inotifywait.cpp common.cpp common.h
inotifywatch_SOURCES = inotifywatch.cpp common.cpp common.h

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

if STATIC_BINARY_FORCE
AM_LDFLAGS = -all-static
endif

install-exec-hook:
	cd $(DESTDIR)$(bindir) && \
	$(LN_S) inotifywait fsnotifywait && \
	$(LN_S) inotifywatch fsnotifywatch

uninstall-hook:
	rm -f $(DESTDIR)$(bindir)/{fsnotifywait,fsnotifywatch}
