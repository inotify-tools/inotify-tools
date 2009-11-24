// FIXME this is cheating!  Make this use only the public API.
#include "../libinotifytools/src/inotifytools_p.h"
#include "../config.h"
#include "common.h"

#include <sys/select.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <regex.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <inotifytools/inotifytools.h>
#include <inotifytools/inotify.h>


extern char *optarg;
extern int optind, opterr, optopt;

#define EXCLUDE_CHUNK 1024

#define nasprintf(...) niceassert( -1 != asprintf(__VA_ARGS__), "out of memory")

// METHODS
bool parse_opts(
  int * argc,
  char *** argv,
  int * events,
  int * timeout,
  int * verbose,
  int * zero,
  int * sort,
  int * recursive,
  char ** fromfile,
  char ** regex,
  char ** iregex
);

void print_help();

static bool done;

void handle_impatient_user( int signal __attribute__((unused)) ) {
	static int times_called = 0;
	if ( times_called ) {
		fprintf( stderr, "No statistics collected, asked to abort before all "
		         "watches could be established.\n" );
		exit(1);
	}
	fprintf( stderr, "No statistics have been collected because I haven't "
	         "finished establishing\n"
	         "inotify watches yet.  If you are sure you want me to exit, "
	         "interrupt me again.\n" );
	++times_called;
}

void handle_signal( int signal __attribute__((unused)) ) {
	done = true;
}

int print_info();

void print_info_now( int signal __attribute__((unused)) ) {
    print_info();
    printf("\n");
}

int events;
int sort;
int zero;

int main(int argc, char ** argv)
{
	events = 0;
	int timeout = 0;
	int verbose = 0;
	zero = 0;
	int recursive = 0;
	char * fromfile = 0;
	sort = -1;
	done = false;
	char * regex = NULL;
	char * iregex = NULL;

	signal( SIGINT, handle_impatient_user );

	// Parse commandline options, aborting if something goes wrong
	if ( !parse_opts( &argc, &argv, &events, &timeout, &verbose, &zero, &sort,
	                 &recursive, &fromfile, &regex, &iregex ) ) {
		return EXIT_FAILURE;
	}

	if (
		(regex && !inotifytools_ignore_events_by_regex(regex, REG_EXTENDED) ) ||
		(iregex && !inotifytools_ignore_events_by_regex(iregex, REG_EXTENDED|
		                                                        REG_ICASE))
	) {
		fprintf(stderr, "Error in `exclude' regular expression.\n");
		return EXIT_FAILURE;
	}

	if ( !inotifytools_initialize() ) {
		fprintf(stderr, "Couldn't initialize inotify.  Are you running Linux "
		                "2.6.13 or later, and was the\n"
		                "CONFIG_INOTIFY option enabled when your kernel was "
		                "compiled?  If so, \n"
		                "something mysterious has gone wrong.  Please e-mail "
		                PACKAGE_BUGREPORT "\n"
		                " and mention that you saw this message.\n");
		return EXIT_FAILURE;
	}

	// Attempt to watch file
	// If events is still 0, make it all events.
	if ( !events )
		events = IN_ALL_EVENTS;

	FileList list = construct_path_list( argc, argv, fromfile );

	if (0 == list.watch_files[0]) {
		fprintf(stderr, "No files specified to watch!\n");
		return EXIT_FAILURE;
	}

	unsigned int num_watches = 0;
	unsigned int status;
	fprintf( stderr, "Establishing watches...\n" );
	for ( int i = 0; list.watch_files[i]; ++i ) {
		char const *this_file = list.watch_files[i];
		if ( recursive && verbose ) {
			fprintf( stderr, "Setting up watch(es) on %s\n", this_file );
		}

		if ( recursive ) {
			status = inotifytools_watch_recursively_with_exclude(
			                               this_file,
			                               events,
			                               list.exclude_files );
		}
		else {
			status = inotifytools_watch_file( this_file, events );
		}
		if ( !status ) {
			if ( inotifytools_error() == ENOSPC ) {
				fprintf(stderr, "Failed to watch %s; upper limit on inotify "
				                "watches reached!\n", this_file );
				fprintf(stderr, "Please increase the amount of inotify watches "
				        "allowed per user via `/proc/sys/fs/inotify/"
				        "max_user_watches'.\n");
			}
			else {
				fprintf(stderr, "Failed to watch %s: %s\n", this_file,
				        strerror( inotifytools_error() ) );
			}
			return EXIT_FAILURE;
		}
		if ( recursive && verbose ) {
			fprintf( stderr, "OK, %s is now being watched.\n", this_file );
		}
	}
	num_watches = inotifytools_get_num_watches();

	if ( verbose ) {
		fprintf( stderr, "Total of %d watches.\n",
		         num_watches );
	}
	fprintf( stderr, "Finished establishing watches, now collecting statistics.\n" );

	if ( timeout && verbose ) {
		fprintf( stderr, "Will listen for events for %d seconds.\n", timeout );
	}

	signal( SIGINT, handle_signal );
	signal( SIGHUP, handle_signal );
	signal( SIGTERM, handle_signal );
	if ( timeout ) {
		signal( SIGALRM, handle_signal );
		alarm( timeout );
	}
        signal( SIGUSR1, print_info_now );

	inotifytools_initialize_stats();
	// Now wait till we get event
	struct inotify_event * event;
	char * moved_from = 0;

	do {
		event = inotifytools_next_event( 0 );
		if ( !event ) {
			if ( !inotifytools_error() ) {
				return EXIT_TIMEOUT;
			}
			else if ( inotifytools_error() != EINTR ) {
				fprintf(stderr, "%s\n", strerror( inotifytools_error() ) );
				return EXIT_FAILURE;
			}
			else {
				continue;
			}
		}

		// if we last had MOVED_FROM and don't currently have MOVED_TO,
		// moved_from file must have been moved outside of tree - so unwatch it.
		if ( moved_from && !(event->mask & IN_MOVED_TO) ) {
			if ( !inotifytools_remove_watch_by_filename( moved_from ) ) {
				fprintf( stderr, "Error removing watch on %s: %s\n",
				         moved_from, strerror(inotifytools_error()) );
			}
			free( moved_from );
			moved_from = 0;
		}

		if ( recursive ) {
			if ((event->mask & IN_CREATE) ||
			    (!moved_from && (event->mask & IN_MOVED_TO))) {
				// New file - if it is a directory, watch it
				static char * new_file;

				nasprintf( &new_file, "%s%s",
				           inotifytools_filename_from_wd( event->wd ),
				           event->name );

				if ( isdir(new_file) &&
				    !inotifytools_watch_recursively( new_file, events ) ) {
					fprintf( stderr, "Couldn't watch new directory %s: %s\n",
					         new_file, strerror( inotifytools_error() ) );
				}
				free( new_file );
			} // IN_CREATE
			else if (event->mask & IN_MOVED_FROM) {
				nasprintf( &moved_from, "%s%s/",
				           inotifytools_filename_from_wd( event->wd ),
				           event->name );
				// if not watched...
				if ( inotifytools_wd_from_filename(moved_from) == -1 ) {
					free( moved_from );
					moved_from = 0;
				}
			} // IN_MOVED_FROM
			else if (event->mask & IN_MOVED_TO) {
				if ( moved_from ) {
					static char * new_name;
					nasprintf( &new_name, "%s%s/",
					           inotifytools_filename_from_wd( event->wd ),
					           event->name );
					inotifytools_replace_filename( moved_from, new_name );
					free( moved_from );
					moved_from = 0;
				} // moved_from
			}
		}

	} while ( !done );
        return print_info();
}

int print_info() {
        unsigned int num_watches = 0;
        num_watches = inotifytools_get_num_watches();

	if ( !inotifytools_get_stat_total( 0 ) ) {
		fprintf( stderr, "No events occurred.\n" );
		return EXIT_SUCCESS;
	}

	// OK, go through the watches and print stats.
	printf("total  ");
	if ( (IN_ACCESS & events) &&
	     ( zero || inotifytools_get_stat_total( IN_ACCESS ) ) )
		printf("access  ");
	if ( ( IN_MODIFY & events) &&
	     ( zero || inotifytools_get_stat_total( IN_MODIFY ) ) )
		printf("modify  ");
	if ( ( IN_ATTRIB & events) &&
	     ( zero || inotifytools_get_stat_total( IN_ATTRIB ) ) )
		printf("attrib  ");
	if ( ( IN_CLOSE_WRITE & events) &&
	     ( zero || inotifytools_get_stat_total( IN_CLOSE_WRITE ) ) )
		printf("close_write  ");
	if ( ( IN_CLOSE_NOWRITE & events) &&
	     ( zero || inotifytools_get_stat_total( IN_CLOSE_NOWRITE ) ) )
		printf("close_nowrite  ");
	if ( ( IN_OPEN & events) &&
	     ( zero || inotifytools_get_stat_total( IN_OPEN ) ) )
		printf("open  ");
	if ( ( IN_MOVED_FROM & events) &&
	     ( zero || inotifytools_get_stat_total( IN_MOVED_FROM ) ) )
		printf("moved_from  ");
	if ( ( IN_MOVED_TO & events) &&
	     ( zero || inotifytools_get_stat_total( IN_MOVED_TO ) ) )
		printf("moved_to  ");
	if ( ( IN_MOVE_SELF & events) &&
	     ( zero || inotifytools_get_stat_total( IN_MOVE_SELF ) ) )
		printf("move_self  ");
	if ( ( IN_CREATE & events) &&
	     ( zero || inotifytools_get_stat_total( IN_CREATE ) ) )
		printf("create  ");
	if ( ( IN_DELETE & events) &&
	     ( zero || inotifytools_get_stat_total( IN_DELETE ) ) )
		printf("delete  ");
	if ( ( IN_DELETE_SELF & events) &&
	     ( zero || inotifytools_get_stat_total( IN_DELETE_SELF ) ) )
		printf("delete_self  ");
	if ( ( IN_UNMOUNT & events) &&
	     ( zero || inotifytools_get_stat_total( IN_UNMOUNT ) ) )
		printf("unmount  ");

	printf("filename\n");

	struct rbtree *tree = inotifytools_wd_sorted_by_event(sort);
	RBLIST *rblist = rbopenlist(tree);
	watch *w = (watch*)rbreadlist(rblist);

	while (w) {
		if ( !zero && !w->hit_total ) {
			w = (watch*)rbreadlist(rblist);
			continue;
		}
		printf("%-5u  ", w->hit_total );
		if ( ( IN_ACCESS & events) &&
		     ( zero || inotifytools_get_stat_total( IN_ACCESS ) ) )
			printf("%-6u  ", w->hit_access );
		if ( ( IN_MODIFY & events) &&
		     ( zero || inotifytools_get_stat_total( IN_MODIFY ) ) )
			printf("%-6u  ", w->hit_modify );
		if ( ( IN_ATTRIB & events) &&
		     ( zero || inotifytools_get_stat_total( IN_ATTRIB ) ) )
			printf("%-6u  ", w->hit_attrib );
		if ( ( IN_CLOSE_WRITE & events) &&
		     ( zero || inotifytools_get_stat_total( IN_CLOSE_WRITE ) ) )
			printf("%-11u  ",w->hit_close_write );
		if ( ( IN_CLOSE_NOWRITE & events) &&
		     ( zero || inotifytools_get_stat_total( IN_CLOSE_NOWRITE ) ) )
			printf("%-13u  ",w->hit_close_nowrite );
		if ( ( IN_OPEN & events) &&
		     ( zero || inotifytools_get_stat_total( IN_OPEN ) ) )
			printf("%-4u  ", w->hit_open );
		if ( ( IN_MOVED_FROM & events) &&
		     ( zero || inotifytools_get_stat_total( IN_MOVED_FROM ) ) )
			printf("%-10u  ", w->hit_moved_from );
		if ( ( IN_MOVED_TO & events) &&
		     ( zero || inotifytools_get_stat_total( IN_MOVED_TO ) ) )
			printf("%-8u  ", w->hit_moved_to );
		if ( ( IN_MOVE_SELF & events) &&
		     ( zero || inotifytools_get_stat_total( IN_MOVE_SELF ) ) )
			printf("%-9u  ", w->hit_move_self );
		if ( ( IN_CREATE & events) &&
		     ( zero || inotifytools_get_stat_total( IN_CREATE ) ) )
			printf("%-6u  ", w->hit_create );
		if ( ( IN_DELETE & events) &&
		     ( zero || inotifytools_get_stat_total( IN_DELETE ) ) )
			printf("%-6u  ", w->hit_delete );
		if ( ( IN_DELETE_SELF & events) &&
		     ( zero || inotifytools_get_stat_total( IN_DELETE_SELF ) ) )
			printf("%-11u  ",w->hit_delete_self );
		if ( ( IN_UNMOUNT & events) &&
		     ( zero || inotifytools_get_stat_total( IN_UNMOUNT ) ) )
			printf("%-7u  ", w->hit_unmount );

		printf("%s\n", w->filename );
		w = (watch*)rbreadlist(rblist);
	}
	rbcloselist(rblist);
	rbdestroy(tree);

	return EXIT_SUCCESS;
}




bool parse_opts(
  int * argc,
  char *** argv,
  int * events,
  int * timeout,
  int * verbose,
  int * zero,
  int * sort,
  int * recursive,
  char ** fromfile,
  char ** regex,
  char ** iregex
) {
	assert( argc ); assert( argv ); assert( events ); assert( timeout );
	assert( verbose ); assert( zero ); assert( sort ); assert( recursive );
	assert( fromfile ); assert( regex ); assert( iregex );

	// Short options
	char * opt_string = "hra:d:zve:t:";

	// Construct array
	struct option long_opts[12];

	// --help
	long_opts[0].name = "help";
	long_opts[0].has_arg = 0;
	long_opts[0].flag = NULL;
	long_opts[0].val = (int)'h';
	// --event
	long_opts[1].name = "event";
	long_opts[1].has_arg = 1;
	long_opts[1].flag = NULL;
	long_opts[1].val = (int)'e';
	int new_event;
	// --timeout
	long_opts[2].name = "timeout";
	long_opts[2].has_arg = 1;
	long_opts[2].flag = NULL;
	long_opts[2].val = (int)'t';
	char * timeout_end = NULL;
	// --verbose
	long_opts[3].name = "verbose";
	long_opts[3].has_arg = 0;
	long_opts[3].flag = NULL;
	long_opts[3].val = (int)'v';
	// --nonzero
	long_opts[4].name = "zero";
	long_opts[4].has_arg = 0;
	long_opts[4].flag = NULL;
	long_opts[4].val = (int)'z';
	// --ascending
	long_opts[5].name = "ascending";
	long_opts[5].has_arg = 1;
	long_opts[5].flag = NULL;
	long_opts[5].val = (int)'a';
	bool sort_set = false;
	// --descending
	long_opts[6].name = "descending";
	long_opts[6].has_arg = 1;
	long_opts[6].flag = NULL;
	long_opts[6].val = (int)'d';
	// --recursive
	long_opts[7].name = "recursive";
	long_opts[7].has_arg = 0;
	long_opts[7].flag = NULL;
	long_opts[7].val = (int)'r';
	// --fromfile
	long_opts[8].name = "fromfile";
	long_opts[8].has_arg = 1;
	long_opts[8].flag = NULL;
	long_opts[8].val = (int)'o';
	// --exclude
	long_opts[9].name = "exclude";
	long_opts[9].has_arg = 1;
	long_opts[9].flag = NULL;
	long_opts[9].val = (int)'c';
	// --excludei
	long_opts[10].name = "excludei";
	long_opts[10].has_arg = 1;
	long_opts[10].flag = NULL;
	long_opts[10].val = (int)'b';
	// Empty last element
	long_opts[11].name = 0;
	long_opts[11].has_arg = 0;
	long_opts[11].flag = 0;
	long_opts[11].val = 0;

	// Get first option
	char curr_opt = getopt_long(*argc, *argv, opt_string, long_opts, NULL);

	// While more options exist...
	while ( (curr_opt != '?') && (curr_opt != (char)-1) )
	{
		switch ( curr_opt )
		{
			// --help or -h
			case 'h':
				print_help();
				// Shouldn't process any further...
				return false;
				break;

			// --verbose or -v
			case 'v':
				++(*verbose);
				break;

			// --recursive or -r
			case 'r':
				++(*recursive);
				break;

			// --zero or -z
			case 'z':
				++(*zero);
				break;

			// --exclude
			case 'c':
				(*regex) = optarg;
				break;

			// --excludei
			case 'b':
				(*iregex) = optarg;
				break;

			// --fromfile
			case 'o':
				if (*fromfile) {
					fprintf(stderr, "Multiple --fromfile options given.\n");
					return false;
				}
				(*fromfile) = optarg;
				break;

			// --timeout or -t
			case 't':
				*timeout = strtoul(optarg, &timeout_end, 10);
				if ( *timeout_end != '\0' || *timeout < 0)
				{
					fprintf(stderr, "'%s' is not a valid timeout value.\n"
					        "Please specify an integer of value 0 or "
					        "greater.\n",
					        optarg);
					return false;
				}
				break;

			// --event or -e
			case 'e':
				// Get event mask from event string
				new_event = inotifytools_str_to_event(optarg);

				// If optarg was invalid, abort
				if ( new_event == -1 ) {
					fprintf(stderr, "'%s' is not a valid event!  Run with the "
					                "'--help' option to see a list of "
					                "events.\n", optarg);
					return false;
				}

				// Add the new event to the event mask
				(*events) = ( (*events) | new_event );

				break;

			// --ascending or -a
			case 'a':
				if ( sort_set ) {
					fprintf( stderr, "Please specify -a or -d once only!\n" );
					return false;
				}
				if ( 0 == strcasecmp( optarg, "total" ) ) {
					(*sort) = 0;
				}
				else if ( 0 == strcasecmp( optarg, "move" ) ) {
					fprintf( stderr, "Cannot sort by `move' event; please use "
					         "`moved_from' or `moved_to'.\n" );
					return false;
				}
				else if ( 0 == strcasecmp( optarg, "close" ) ) {
					fprintf( stderr, "Cannot sort by `close' event; please use "
					         "`close_write' or `close_nowrite'.\n" );
					return false;
				}
				else {
					int event = inotifytools_str_to_event(optarg);

					// If optarg was invalid, abort
					if ( event == -1 ) {
						fprintf(stderr, "'%s' is not a valid key for "
						        "sorting!\n", optarg);
						return false;
					}

					(*sort) = event;
				}
				sort_set = true;
				break;


			// --descending or -d
			case 'd':
				if ( sort_set ) {
					fprintf( stderr, "Please specify -a or -d once only!\n" );
					return false;
				}
				if ( 0 == strcasecmp( optarg, "total" ) ) {
					(*sort) = -1;
				}
				else {
					int event = inotifytools_str_to_event(optarg);

					// If optarg was invalid, abort
					if ( event == -1 ) {
						fprintf(stderr, "'%s' is not a valid key for "
						        "sorting!\n", optarg);
						return false;
					}

					(*sort) = -event;
				}
				break;

		}

		curr_opt = getopt_long(*argc, *argv, opt_string, long_opts, NULL);

	}

	(*argc) -= optind;
	*argv = &(*argv)[optind];

	if ( (*sort) != 0 && (*sort) != -1 &&
	     !(abs(*sort) & ((*events) ? (*events) : IN_ALL_EVENTS) )) {
		fprintf( stderr, "Can't sort by an event which isn't being watched "
		         "for!\n" );
		return false;
	}

	if ( *regex && *iregex ) {
		fprintf(stderr, "--exclude and --excludei cannot both be specified.\n");
		return false;
	}

	// If ? returned, invalid option
	return (curr_opt != '?');
}


void print_help()
{
	printf("inotifywatch %s\n", PACKAGE_VERSION);
	printf("Gather filesystem usage statistics using inotify.\n");
	printf("Usage: inotifywatch [ options ] file1 [ file2 ] [ ... ]\n");
	printf("Options:\n");
	printf("\t-h|--help    \tShow this help text.\n");
	printf("\t-v|--verbose \tBe verbose.\n");
	printf("\t@<file>       \tExclude the specified file from being "
	       "watched.\n");
	printf("\t--fromfile <file>\n"
	       "\t\tRead files to watch from <file> or `-' for stdin.\n");
	printf("\t--exclude <pattern>\n"
	       "\t\tExclude all events on files matching the extended regular\n"
	       "\t\texpression <pattern>.\n");
	printf("\t--excludei <pattern>\n"
	       "\t\tLike --exclude but case insensitive.\n");
	printf("\t-z|--zero\n"
	       "\t\tIn the final table of results, output rows and columns even\n"
	       "\t\tif they consist only of zeros (the default is to not output\n"
	       "\t\tthese rows and columns).\n");
	printf("\t-r|--recursive\tWatch directories recursively.\n");
	printf("\t-t|--timeout <seconds>\n"
	       "\t\tListen only for specified amount of time in seconds; if\n"
	       "\t\tomitted or 0, inotifywatch will execute until receiving an\n"
	       "\t\tinterrupt signal.\n");
	printf("\t-e|--event <event1> [ -e|--event <event2> ... ]\n"
	       "\t\tListen for specific event(s).  If omitted, all events are \n"
	       "\t\tlistened for.\n");
	printf("\t-a|--ascending <event>\n"
	       "\t\tSort ascending by a particular event, or `total'.\n");
	printf("\t-d|--descending <event>\n"
	       "\t\tSort descending by a particular event, or `total'.\n\n");
	printf("Exit status:\n");
	printf("\t%d  -  Exited normally.\n", EXIT_SUCCESS);
	printf("\t%d  -  Some error occurred.\n\n", EXIT_FAILURE);
	printf("Events:\n");
	print_event_descriptions();
}

