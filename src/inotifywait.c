#include "../config.h"
#include "common.h"

#include <sys/types.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/time.h>

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <regex.h>
#include <signal.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <syslog.h>
#include <unistd.h>

#include <inotifytools/inotifytools.h>
#include <inotifytools/inotify.h>

extern char *optarg;
extern int optind, opterr, optopt;

#define MAX_STRLEN 4096

#define nasprintf(...) niceassert( -1 != asprintf(__VA_ARGS__), "out of memory")

// METHODS
bool parse_opts(
  int * argc,
  char *** argv,
  int * events,
  bool * monitor,
  int * quiet,
  long int * timeout,
  int * recursive,
  bool * csv,
  bool * daemon,
  bool * syslog,
  char ** format,
  char ** timefmt,
  char ** fromfile,
  char ** outfile,
  char ** exc_regex,
  char ** exc_iregex,
  char ** inc_regex,
  char ** inc_iregex
);

void print_help();


char * csv_escape( char * string ) {
	static char csv[MAX_STRLEN+1];
	static unsigned int i, ind;

	if (string == NULL) {
	  return NULL;
	}

	if ( strlen(string) > MAX_STRLEN ) {
		return NULL;
	}

	if ( strlen(string) == 0 ) {
		return NULL;
	}

	// May not need escaping
	if ( !strchr(string, '"') && !strchr(string, ',') && !strchr(string, '\n')
	     && string[0] != ' ' && string[strlen(string)-1] != ' ' ) {
		strcpy( csv, string );
		return csv;
	}

	// OK, so now we _do_ need escaping.
	csv[0] = '"';
	ind = 1;
	for ( i = 0; i < strlen(string); ++i ) {
		if ( string[i] == '"' ) {
			csv[ind++] = '"';
		}
		csv[ind++] = string[i];
	}
	csv[ind++] = '"';
	csv[ind] = '\0';

	return csv;
}


void validate_format( char * fmt ) {
	// Make a fake event
	struct inotify_event * event =
	   (struct inotify_event *)malloc(sizeof(struct inotify_event) + 4);
	if ( !event ) {
		fprintf( stderr, "Seem to be out of memory... yikes!\n" );
		exit(EXIT_FAILURE);
	}
	event->wd = 0;
	event->mask = IN_ALL_EVENTS;
	event->len = 3;
	strcpy( event->name, "foo" );
	FILE * devnull = fopen( "/dev/null", "a" );
	if ( !devnull ) {
		fprintf( stderr, "Couldn't open /dev/null: %s\n", strerror(errno) );
		free( event );
		return;
	}
	if ( -1 == inotifytools_fprintf( devnull, event, fmt ) ) {
		fprintf( stderr, "Something is wrong with your format string.\n" );
		exit(EXIT_FAILURE);
	}
	free( event );
	fclose(devnull);
}


void output_event_csv( struct inotify_event * event ) {
	char *filename = csv_escape(inotifytools_filename_from_wd(event->wd));

	if (filename != NULL)
		printf("%s,", filename);

	printf("%s,", csv_escape( inotifytools_event_to_str( event->mask ) ) );
	if ( event->len > 0 )
		printf("%s", csv_escape( event->name ) );
	printf("\n");
}


void output_error( bool syslog, char* fmt, ... ) {
	va_list va;
	va_start(va, fmt);
	if ( syslog ) {
		vsyslog(LOG_INFO, fmt, va);
	} else {
		vfprintf(stderr, fmt, va);
	}
	va_end(va);
}


int main(int argc, char ** argv)
{
	int events = 0;
        int orig_events;
	bool monitor = false;
	int quiet = 0;
	long int timeout = BLOCKING_TIMEOUT;
	int recursive = 0;
	bool csv = false;
	bool dodaemon = false;
	bool syslog = false;
	char * format = NULL;
	char * timefmt = NULL;
	char * fromfile = NULL;
	char * outfile = NULL;
	char * exc_regex = NULL;
	char * exc_iregex = NULL;
	char * inc_regex = NULL;
	char * inc_iregex = NULL;
    int fd;

	// Parse commandline options, aborting if something goes wrong
	if ( !parse_opts(&argc, &argv, &events, &monitor, &quiet, &timeout,
	                 &recursive, &csv, &dodaemon, &syslog, &format, &timefmt,
                         &fromfile, &outfile,
                         &exc_regex, &exc_iregex, &inc_regex, &inc_iregex) ) {
		return EXIT_FAILURE;
	}

	if ( !inotifytools_initialize() ) {
		warn_inotify_init_error();
		return EXIT_FAILURE;
	}

	if ( timefmt ) inotifytools_set_printf_timefmt( timefmt );
	if (
		(exc_regex && !inotifytools_ignore_events_by_regex(exc_regex, REG_EXTENDED) ) ||
		(exc_iregex && !inotifytools_ignore_events_by_regex(exc_iregex, REG_EXTENDED|
		                                                        REG_ICASE))
	) {
		fprintf(stderr, "Error in `exclude' regular expression.\n");
		return EXIT_FAILURE;
	}
	if (
		(inc_regex && !inotifytools_ignore_events_by_inverted_regex(inc_regex, REG_EXTENDED) ) ||
		(inc_iregex && !inotifytools_ignore_events_by_inverted_regex(inc_iregex, REG_EXTENDED|
		                                                        REG_ICASE))
	) {
		fprintf(stderr, "Error in `include' regular expression.\n");
		return EXIT_FAILURE;
	}


	if ( format ) validate_format(format);

	// Attempt to watch file
	// If events is still 0, make it all events.
	if (events == 0)
		events = IN_ALL_EVENTS;
        orig_events = events;
        if ( monitor && recursive ) {
                events = events | IN_CREATE | IN_MOVED_TO | IN_MOVED_FROM;
        }

	FileList list = construct_path_list( argc, argv, fromfile );

	if (0 == list.watch_files[0]) {
		fprintf(stderr, "No files specified to watch!\n");
		return EXIT_FAILURE;
	}


    // Daemonize - BSD double-fork approach
	if ( dodaemon ) {
#ifdef HAVE_DAEMON
        if (daemon(0, 0)) {
            fprintf(stderr, "Failed to daemonize!\n");
            return EXIT_FAILURE;
        }
#else
            pid_t pid = fork();
	        if (pid < 0) {
			fprintf(stderr, "Failed to fork1 whilst daemonizing!\n");
	                return EXIT_FAILURE;
	        } 
	        if (pid > 0) {
			_exit(0);
	        }
		if (setsid() < 0) {
			fprintf(stderr, "Failed to setsid whilst daemonizing!\n");
	                return EXIT_FAILURE;
	        }
		signal(SIGHUP,SIG_IGN);
	        pid = fork();
	        if (pid < 0) {
	                fprintf(stderr, "Failed to fork2 whilst daemonizing!\n");
	                return EXIT_FAILURE;
	        }
	        if (pid > 0) {
	                _exit(0);
	        }
		if (chdir("/") < 0) {
			fprintf(stderr, "Failed to chdir whilst daemonizing!\n");
	                return EXIT_FAILURE;
	        }
#endif

		// Redirect stdin from /dev/null
	        fd = open("/dev/null", O_RDONLY);
		if (fd != fileno(stdin)) {
			dup2(fd, fileno(stdin));
			close(fd);
		}

		// Redirect stdout to a file
	        fd = open(outfile, O_WRONLY | O_CREAT | O_APPEND, 0600);
		if (fd < 0) {
                        fprintf( stderr, "Failed to open output file %s\n", outfile );
                        return EXIT_FAILURE;
                }
		if (fd != fileno(stdout)) {
			dup2(fd, fileno(stdout));
			close(fd);
		}

        // Redirect stderr to /dev/null
		fd = open("/dev/null", O_WRONLY);
	        if (fd != fileno(stderr)) {
	                dup2(fd, fileno(stderr));
	                close(fd);
	        }
	
        } else if (outfile != NULL) { // Redirect stdout to a file if specified
		fd = open(outfile, O_WRONLY | O_CREAT | O_APPEND, 0600);
		if (fd < 0) {
			fprintf( stderr, "Failed to open output file %s\n", outfile );
			return EXIT_FAILURE;
		}
		if (fd != fileno(stdout)) {
			dup2(fd, fileno(stdout));
			close(fd);
		}
        }

        if ( syslog ) {
		openlog ("inotifywait", LOG_CONS | LOG_PID | LOG_NDELAY, LOG_DAEMON);
        }

	if ( !quiet ) {
		if ( recursive ) {
			output_error( syslog, "Setting up watches.  Beware: since -r "
				"was given, this may take a while!\n" );
		} else {
			output_error( syslog, "Setting up watches.\n" );
		}
	}

	// now watch files
	for ( int i = 0; list.watch_files[i]; ++i ) {
		char const *this_file = list.watch_files[i];
		if ( (recursive && !inotifytools_watch_recursively_with_exclude(
		                        this_file,
		                        events,
		                        list.exclude_files ))
		     || (!recursive && !inotifytools_watch_file( this_file, events )) ){
			if ( inotifytools_error() == ENOSPC ) {
				output_error( syslog, "Failed to watch %s; upper limit on inotify "
				                "watches reached!\n", this_file );
				output_error( syslog, "Please increase the amount of inotify watches "
				        "allowed per user via `/proc/sys/fs/inotify/"
				        "max_user_watches'.\n");
			}
			else {
				output_error( syslog, "Couldn't watch %s: %s\n", this_file,
				        strerror( inotifytools_error() ) );
			}
			return EXIT_FAILURE;
		}
	}

	if ( !quiet ) {
		output_error( syslog, "Watches established.\n" );
	}

	// Now wait till we get event
	struct inotify_event * event;
	char * moved_from = 0;

	do {
		event = inotifytools_next_event( timeout );
		if ( !event ) {
			if ( !inotifytools_error() ) {
				return EXIT_TIMEOUT;
			}
			else {
				output_error( syslog, "%s\n", strerror( inotifytools_error() ) );
				return EXIT_FAILURE;
			}
		}

		if ( quiet < 2 && (event->mask & orig_events) ) {
			if ( csv ) {
				output_event_csv( event );
			}
			else if ( format ) {
				inotifytools_printf( event, format );
			}
			else {
				inotifytools_printf( event, "%w %,e %f\n" );
			}
		}

		// if we last had MOVED_FROM and don't currently have MOVED_TO,
		// moved_from file must have been moved outside of tree - so unwatch it.
		if ( moved_from && !(event->mask & IN_MOVED_TO) ) {
			if ( !inotifytools_remove_watch_by_filename( moved_from ) ) {
				output_error( syslog, "Error removing watch on %s: %s\n",
				         moved_from, strerror(inotifytools_error()) );
			}
			free( moved_from );
			moved_from = 0;
		}

		if ( monitor && recursive ) {
			if ((event->mask & IN_CREATE) ||
			    (!moved_from && (event->mask & IN_MOVED_TO))) {
				// New file - if it is a directory, watch it
				static char * new_file;

				nasprintf( &new_file, "%s%s",
				           inotifytools_filename_from_wd( event->wd ),
				           event->name );

				if ( isdir(new_file) &&
				    !inotifytools_watch_recursively( new_file, events ) ) {
					output_error( syslog, "Couldn't watch new directory %s: %s\n",
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

		fflush( NULL );

	} while ( monitor );

	// If we weren't trying to listen for this event...
	if ( (events & event->mask) == 0 ) {
		// ...then most likely something bad happened, like IGNORE etc.
		return EXIT_FAILURE;
	}

	return EXIT_SUCCESS;
}


bool parse_opts(
  int * argc,
  char *** argv,
  int * events,
  bool * monitor,
  int * quiet,
  long int * timeout,
  int * recursive,
  bool * csv,
  bool * daemon,
  bool * syslog,
  char ** format,
  char ** timefmt,
  char ** fromfile,
  char ** outfile,
  char ** exc_regex,
  char ** exc_iregex,
  char ** inc_regex,
  char ** inc_iregex
) {
	assert( argc ); assert( argv ); assert( events ); assert( monitor );
	assert( quiet ); assert( timeout ); assert( csv ); assert( daemon );
	assert( syslog ); assert( format ); assert( timefmt ); assert( fromfile ); 
	assert( outfile ); assert( exc_regex ); assert( exc_iregex );
	assert( inc_regex ); assert( inc_iregex );

    // Settings for options
	int new_event;

	// How many times --exclude has been specified
	unsigned int exclude_count = 0;

	// How many times --excludei has been specified
	unsigned int excludei_count = 0;

	const char *regex_warning = "only the last option will be taken into consideration.\n";

	// format with trailing newline
	static char * newlineformat;

	// Short options
	static const char opt_string[] = "mrhcdsqt:fo:e:";

    // Long options
    static const struct option long_opts[] = {
        {"help",              no_argument, NULL, 'h'},
        {"event",       required_argument, NULL, 'e'},
        {"monitor",           no_argument, NULL, 'm'},
        {"quiet",             no_argument, NULL, 'q'},
        {"timeout",     required_argument, NULL, 't'},
        {"filename",          no_argument, NULL, 'f'},
        {"recursive",         no_argument, NULL, 'r'},
        {"csv",               no_argument, NULL, 'c'},
        {"daemon",            no_argument, NULL, 'd'},
        {"syslog",            no_argument, NULL, 's'},
        {"format",      required_argument, NULL, 'n'},
        {"timefmt",     required_argument, NULL, 'i'},
        {"fromfile",    required_argument, NULL, 'z'},
        {"outfile",     required_argument, NULL, 'o'},
        {"exclude",     required_argument, NULL, 'a'},
        {"excludei",    required_argument, NULL, 'b'},
        {"include",     required_argument, NULL, 'j'},
        {"includei",    required_argument, NULL, 'k'},
        {NULL, 0, 0, 0},
    };

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

			// --monitor or -m
			case 'm':
				*monitor = true;
				break;

			// --quiet or -q
			case 'q':
				(*quiet)++;
				break;

			// --recursive or -r
			case 'r':
				(*recursive)++;
				break;

			// --csv or -c
			case 'c':
				(*csv) = true;
				break;

			// --daemon or -d
			case 'd':
				(*daemon) = true;
				(*monitor) = true;
				(*syslog) = true;
				break;

			// --syslog or -s
			case 's':
				(*syslog) = true;
				break;

			// --filename or -f
			case 'f':
				fprintf(stderr, "The '--filename' option no longer exists.  "
				                "The option it enabled in earlier\nversions of "
				                "inotifywait is now turned on by default.\n");
				return false;
				break;

			// --format
			case 'n':
				newlineformat = (char *)malloc(strlen(optarg)+2);
				strcpy( newlineformat, optarg );
				strcat( newlineformat, "\n" );
				(*format) = newlineformat;
				break;

			// --timefmt
			case 'i':
				(*timefmt) = optarg;
				break;

			// --exclude
			case 'a':
				(*exc_regex) = optarg;
				exclude_count++;
				break;

			// --excludei
			case 'b':
				(*exc_iregex) = optarg;
				excludei_count++;
				break;

			// --include
			case 'j':
				(*inc_regex) = optarg;
				break;

			// --includei
			case 'k':
				(*inc_iregex) = optarg;
				break;

			// --fromfile
			case 'z':
				if (*fromfile) {
					fprintf(stderr, "Multiple --fromfile options given.\n");
					return false;
				}
				(*fromfile) = optarg;
				break;

			// --outfile
			case 'o':
				if (*outfile) {
					fprintf(stderr, "Multiple --outfile options given.\n");
					return false;
				}
				(*outfile) = optarg;
				break;

			// --timeout or -t
			case 't':
			  if (!is_timeout_option_valid(timeout, optarg)) {
			    return false;
			  }
			  break;

			// --event or -e
			case 'e':
				// Get event mask from event string
				new_event = inotifytools_str_to_event(optarg);

				// If optarg was invalid, abort
				if ( new_event == -1 )
				{
					fprintf(stderr, "'%s' is not a valid event!  Run with the "
					                "'--help' option to see a list of "
					                "events.\n", optarg);
					return false;
				}

				// Add the new event to the event mask
				(*events) = ( (*events) | new_event );

				break;


		}

		curr_opt = getopt_long(*argc, *argv, opt_string, long_opts, NULL);

	}

	if ( *exc_regex && *exc_iregex ) {
		fprintf(stderr, "--exclude and --excludei cannot both be specified.\n");
		return false;
	}

	if ( *inc_regex && *inc_iregex ) {
		fprintf(stderr, "--include and --includei cannot both be specified.\n");
		return false;
	}

	if ( (*inc_regex && *exc_regex) || (*inc_regex && *exc_iregex) ||
		 (*inc_iregex && *exc_regex) || (*inc_iregex && *exc_iregex)) {
		fprintf(stderr, "include and exclude regexp cannot both be specified.\n");
		return false;
	}

	if ( *format && *csv ) {
		fprintf(stderr, "-c and --format cannot both be specified.\n");
		return false;
	}

	if ( !*format && *timefmt ) {
		fprintf(stderr, "--timefmt cannot be specified without --format.\n");
		return false;
	}

	if ( *format && strstr( *format, "%T" ) && !*timefmt ) {
		fprintf(stderr, "%%T is in --format string, but --timefmt was not "
		                "specified.\n");
		return false;
	}

	if ( *daemon && *outfile == NULL ) {
		fprintf(stderr, "-o must be specified with -d.\n");
		return false;
	}

	if (exclude_count > 1) {
	  fprintf(stderr, "--exclude: %s", regex_warning);
	}

	if (excludei_count > 1) {
	  fprintf(stderr, "--excludei: %s", regex_warning);
	}

	(*argc) -= optind;
	*argv = &(*argv)[optind];

	// If ? returned, invalid option
	return (curr_opt != '?');
}


void print_help()
{
	printf("inotifywait %s\n", PACKAGE_VERSION);
	printf("Wait for a particular event on a file or set of files.\n");
	printf("Usage: inotifywait [ options ] file1 [ file2 ] [ file3 ] "
	       "[ ... ]\n");
	printf("Options:\n");
	printf("\t-h|--help     \tShow this help text.\n");
	printf("\t@<file>       \tExclude the specified file from being "
	       "watched.\n");
	printf("\t--exclude <pattern>\n"
	       "\t              \tExclude all events on files matching the\n"
	       "\t              \textended regular expression <pattern>.\n"
	       "\t              \tOnly the last --exclude option will be\n"
	       "\t              \ttaken into consideration.\n");
	printf("\t--excludei <pattern>\n"
	       "\t              \tLike --exclude but case insensitive.\n");
	printf("\t--include <pattern>\n"
	       "\t              \tExclude all events on files except the ones\n"
	       "\t              \tmatching the extended regular expression\n"
	       "\t              \t<pattern>.\n");
	printf("\t--includei <pattern>\n"
	       "\t              \tLike --include but case insensitive.\n");
	printf("\t-m|--monitor  \tKeep listening for events forever or until --timeout expires.\n"
	       "\t              \tWithout this option, inotifywait will exit after one event is received.\n");
	printf("\t-d|--daemon   \tSame as --monitor, except run in the background\n"
               "\t              \tlogging events to a file specified by --outfile.\n"
               "\t              \tImplies --syslog.\n");
	printf("\t-r|--recursive\tWatch directories recursively.\n");
	printf("\t--fromfile <file>\n"
	       "\t              \tRead files to watch from <file> or `-' for "
	       "stdin.\n");
	printf("\t-o|--outfile <file>\n"
	       "\t              \tPrint events to <file> rather than stdout.\n");
	printf("\t-s|--syslog   \tSend errors to syslog rather than stderr.\n");
	printf("\t-q|--quiet    \tPrint less (only print events).\n");
	printf("\t-qq           \tPrint nothing (not even events).\n");
	printf("\t--format <fmt>\tPrint using a specified printf-like format\n"
	       "\t              \tstring; read the man page for more details.\n");
	printf("\t--timefmt <fmt>\tstrftime-compatible format string for use with\n"
	       "\t              \t%%T in --format string.\n");
	printf("\t-c|--csv      \tPrint events in CSV format.\n");
	printf("\t-t|--timeout <seconds>\n"
	       "\t              \tWhen listening for a single event, time out "
	       "after\n"
	       "\t              \twaiting for an event for <seconds> seconds.\n"
	       "\t              \tIf <seconds> is negative, inotifywait will never time "
	       "out.\n");
	printf("\t-e|--event <event1> [ -e|--event <event2> ... ]\n"
	       "\t\tListen for specific event(s).  If omitted, all events are \n"
	       "\t\tlistened for.\n\n");
	printf("Exit status:\n");
	printf("\t%d  -  An event you asked to watch for was received.\n",
	       EXIT_SUCCESS );
	printf("\t%d  -  An event you did not ask to watch for was received\n",
	       EXIT_FAILURE);
	printf("\t      (usually delete_self or unmount), or some error "
	       "occurred.\n");
	printf("\t%d  -  The --timeout option was given and no events occurred\n",
	       EXIT_TIMEOUT);
	printf("\t      in the specified interval of time.\n\n");
	printf("Events:\n");
	print_event_descriptions();
}

