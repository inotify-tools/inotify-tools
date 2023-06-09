#include <stdio.h>
#include <string.h>
#include <inotifytools/inotifytools.h>
#include <inotifytools/inotify.h>

/*
 * libinotifytools example program.
 * Compile with gcc -linotifytools example.c
 */
int main() {
	// initialize and watch the entire directory tree from the current working
	// directory downwards for all events
	if ( !inotifytools_initialize()
	  || !inotifytools_watch_recursively( ".", IN_ALL_EVENTS ) ) {
		fprintf(stderr, "%s\n", strerror( inotifytools_error() ) );
		return -1;
	}

	// set time format to 24 hour time, HH:MM:SS
	inotifytools_set_printf_timefmt( "%T" );

	// Output all events as "<timestamp> <path> <events>"
	struct inotify_event * event = inotifytools_next_event( -1 );
	while ( event ) {
		inotifytools_printf( event, "%T %w%f %e\n" );
		event = inotifytools_next_event( -1 );
	}
}

