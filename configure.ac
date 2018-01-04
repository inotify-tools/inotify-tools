# -*- Autoconf -*-
# Process this file with autoconf to produce a configure script.

AC_PREREQ(2.59)
AC_INIT([inotify-tools], [3.20.0], [radu.voicilas@gmail.com])
AC_CONFIG_AUX_DIR([config])
AC_CONFIG_SRCDIR([src/inotifywait.c])
AC_CONFIG_HEADERS([config.h])
AC_CONFIG_HEADERS([libinotifytools/src/inotifytools/inotify.h])
AC_CONFIG_MACRO_DIR([m4])
AC_DEFINE([_GNU_SOURCE], [], [For a few GNU-specific functions])
AC_PROG_MAKE_SET

# Checks for programs.
AC_PROG_CC
AM_INIT_AUTOMAKE
LT_INIT

AC_PATH_PROG(DOXYGEN, doxygen, NO_DOXYGEN)

AC_ARG_ENABLE(doxygen,
    AS_HELP_STRING([--enable-doxygen],[enable libinotifytools API docs generation]),
    DOXYGEN_ENABLE=$enableval,
   [if test "$DOXYGEN" != NO_DOXYGEN; then
        DOXYGEN_ENABLE=yes
    else
        DOXYGEN_ENABLE=no
    fi]
)

if test "$DOXYGEN_ENABLE" = "yes" && test "$DOXYGEN" = "NO_DOXYGEN"; then
	AC_MSG_ERROR([Cannot find doxygen!  Make sure it is in PATH.])
fi

if test "$DOXYGEN_ENABLE" = "no"; then
	DOXYGEN=NO_DOXYGEN
fi
AM_CONDITIONAL([DOXYGEN_ENABLE], test x$DOXYGEN != xNO_DOXYGEN )

AC_ARG_ENABLE(static-binary,
    AS_HELP_STRING([--enable-static-binary],[enable static linking of binaries]),
    STATIC_BINARY_ENABLE=$enableval, STATIC_BINARY_ENABLE=no)
AM_CONDITIONAL([STATIC_BINARY_ENABLE], test "$STATIC_BINARY_ENABLE" = "yes")


# Checks for libraries.

# Checks for header files.
AC_CHECK_HEADERS([sys/inotify.h mcheck.h])
AC_LANG(C)
AC_MSG_CHECKING([whether sys/inotify.h actually works])
AC_COMPILE_IFELSE(
  [AC_LANG_PROGRAM([[#include <sys/inotify.h>]],
                  [[return (-1 == inotify_init());]]
  )],
  [AC_MSG_RESULT([yup]); AC_DEFINE([SYS_INOTIFY_H_EXISTS_AND_WORKS],[1],[sys/inotify.h exists and works correctly])],
  [AC_MSG_RESULT([nope, using own inotify headers])]
)

# Checks for typedefs, structures, and compiler characteristics.
AC_C_CONST
AC_C_INLINE

# Checks for library functions.
AC_CHECK_FUNCS([daemon])

# Set variables used in man page templates
MAN_DATE=$(date +'%B %d, %Y')
MAN_PACKAGE_VERSION=$PACKAGE_VERSION
AC_SUBST([MAN_DATE])
AC_SUBST([MAN_PACKAGE_VERSION])

# Output...
AC_CONFIG_FILES([
  Makefile
  src/Makefile
  man/Makefile
  man/inotifywait.1
  man/inotifywatch.1
  libinotifytools/Makefile
  libinotifytools/src/Makefile
  libinotifytools/src/inotifytools/Makefile
])
AC_OUTPUT
