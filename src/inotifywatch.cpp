#include "../config.h"
#include "../libinotifytools/src/inotifytools_p.h"
#include "common.h"

#include <sys/select.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <limits.h>
#include <regex.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <libgen.h>

#include <inotifytools/inotify.h>
#include <inotifytools/inotifytools.h>

extern char* optarg;
extern int optind, opterr, optopt;

// METHODS
static bool parse_opts(int* argc,
		       char*** argv,
		       int* events,
		       long* timeout,
		       int* verbose,
		       int* zero,
		       int* sort,
		       int* recursive,
		       int* no_dereference,
		       char** fromfile,
		       char** exc_regex,
		       char** exc_iregex,
		       char** inc_regex,
		       char** inc_iregex,
		       int* fanotify,
		       bool* filesystem);

void print_help(const char *tool_name);

static bool done;

void handle_impatient_user(int signal __attribute__((unused))) {
	static int times_called = 0;
	if (times_called) {
		fprintf(stderr,
			"No statistics collected, asked to abort before all "
			"watches could be established.\n");
		_exit(1);
	}

	fprintf(stderr,
		"No statistics have been collected because I haven't "
		"finished establishing\n"
		"inotify watches yet.  If you are sure you want me to exit, "
		"interrupt me again.\n");
	++times_called;
}

void handle_signal(int signal __attribute__((unused))) {
	done = true;
}

int print_info();

void print_info_now(int signal __attribute__((unused))) {
	print_info();
	printf("\n");
}

int events;
int sort;
int zero;

int main(int argc, char** argv) {
	events = 0;
	long timeout = BLOCKING_TIMEOUT;
	int verbose = 0;
	zero = 0;
	int recursive = 0;
	int fanotify = 0;
	bool filesystem = false;
	int no_dereference = 0;
	char* fromfile = 0;
	sort = -1;
	done = false;
	char* exc_regex = NULL;
	char* exc_iregex = NULL;
	char* inc_regex = NULL;
	char* inc_iregex = NULL;
	int rc;

	if ((argc > 0) && (strncmp(basename(argv[0]), "fsnotify", 8) == 0)) {
		// Default to fanotify for the fsnotify* tools.
		fanotify = 1;
	}

	signal(SIGINT, handle_impatient_user);

	// Parse commandline options, aborting if something goes wrong
	if (!parse_opts(&argc, &argv, &events, &timeout, &verbose, &zero, &sort,
			&recursive, &no_dereference, &fromfile, &exc_regex,
			&exc_iregex, &inc_regex, &inc_iregex, &fanotify,
			&filesystem)) {
		return EXIT_FAILURE;
	}

	if ((exc_regex &&
	     !inotifytools_ignore_events_by_regex(exc_regex, REG_EXTENDED, recursive)) ||
	    (exc_iregex && !inotifytools_ignore_events_by_regex(
			       exc_iregex, REG_EXTENDED | REG_ICASE, recursive))) {
		fprintf(stderr, "Error in `exclude' regular expression.\n");
		return EXIT_FAILURE;
	}

	if ((inc_regex && !inotifytools_ignore_events_by_inverted_regex(
			      inc_regex, REG_EXTENDED, recursive)) ||
	    (inc_iregex && !inotifytools_ignore_events_by_inverted_regex(
			       inc_iregex, REG_EXTENDED | REG_ICASE, recursive))) {
		fprintf(stderr, "Error in `include' regular expression.\n");
		return EXIT_FAILURE;
	}

	rc = inotifytools_init(fanotify, filesystem, verbose);
	if (!rc) {
		warn_inotify_init_error(fanotify);
		return EXIT_FAILURE;
	}

	// Attempt to watch file
	// If events is still 0, make it all events.
	if (!events)
		events = IN_ALL_EVENTS;
	if (no_dereference)
		events = events | IN_DONT_FOLLOW;

	if (fanotify)
		events |= IN_ISDIR;

	FileList list(argc, argv);
	construct_path_list(argc, argv, fromfile, &list);

	if (0 == list.watch_files_[0]) {
		fprintf(stderr, "No files specified to watch!\n");
		return EXIT_FAILURE;
	}

	unsigned int num_watches = 0;
	unsigned int status;
	fprintf(stderr, "Establishing watches...\n");
	for (int i = 0; list.watch_files_[i]; ++i) {
		char const* this_file = list.watch_files_[i];
		if (filesystem) {
			fprintf(stderr, "Setting up filesystem watch on %s\n",
				this_file);
			if (!inotifytools_watch_files(list.watch_files_,
						      events)) {
				fprintf(
				    stderr,
				    "Couldn't add filesystem watch %s: %s\n",
				    this_file, strerror(inotifytools_error()));
				return EXIT_FAILURE;
			}
			break;
		}

		if (recursive && verbose) {
			fprintf(stderr, "Setting up watch(es) on %s\n",
				this_file);
		}

		if (recursive) {
			status = inotifytools_watch_recursively_with_exclude(
			    this_file, events, list.exclude_files_);
		} else {
			status = inotifytools_watch_file(this_file, events);
		}
		if (!status) {
			if (inotifytools_error() == ENOSPC) {
				const char* backend =
				    fanotify ? "fanotify" : "inotify";
				const char* resource =
				    fanotify ? "marks" : "watches";
				fprintf(
				    stderr,
				    "Failed to watch %s; upper limit on %s %s "
				    "reached!\n",
				    this_file, backend, resource);
				fprintf(stderr,
					"Please increase the amount of %s %s "
					"allowed per user via `/proc/sys/fs/%s/"
					"max_user_%s'.\n",
					backend, resource, backend, resource);
			} else {
				fprintf(stderr, "Failed to watch %s: %s\n",
					this_file,
					strerror(inotifytools_error()));
			}

			return EXIT_FAILURE;
		}
		if (recursive && verbose) {
			fprintf(stderr, "OK, %s is now being watched.\n",
				this_file);
		}
	}
	num_watches = inotifytools_get_num_watches();

	if (verbose) {
		fprintf(stderr, "Total of %u watches.\n", num_watches);
	}
	fprintf(stderr,
		"Finished establishing watches, now collecting statistics.\n");

	if (timeout < 0) {
		// Used to test filesystem support for inotify/fanotify
		fprintf(stderr, "Negative timeout specified - abort!\n");
		return EXIT_FAILURE;
	}
	if (timeout && verbose) {
		fprintf(stderr, "Will listen for events for %lu seconds.\n",
			timeout);
	}

	signal(SIGINT, handle_signal);
	signal(SIGHUP, handle_signal);
	signal(SIGTERM, handle_signal);
	if (timeout) {
		signal(SIGALRM, handle_signal);
		alarm(timeout);
	} else {
		alarm(UINT_MAX);
	}

	signal(SIGUSR1, print_info_now);

	inotifytools_initialize_stats();
	// Now wait till we get event
	struct inotify_event* event;
	char* moved_from = 0;

	do {
		event = inotifytools_next_event(BLOCKING_TIMEOUT);
		if (!event) {
			if (!inotifytools_error()) {
				return EXIT_TIMEOUT;
			} else if (inotifytools_error() != EINTR) {
				fprintf(stderr, "%s\n",
					strerror(inotifytools_error()));

				return EXIT_FAILURE;
			} else {
				continue;
			}
		}

		// TODO: replace filename of renamed filesystem watch entries
		if (filesystem)
			continue;

		// if we last had MOVED_FROM and don't currently have MOVED_TO,
		// moved_from file must have been moved outside of tree - so
		// unwatch it.
		if (moved_from && !(event->mask & IN_MOVED_TO)) {
			if (!inotifytools_remove_watch_by_filename(
				moved_from)) {
				fprintf(
				    stderr, "Error removing watch on %s: %s\n",
				    moved_from, strerror(inotifytools_error()));
			}
			free(moved_from);
			moved_from = 0;
		}

		if (recursive) {
			if ((event->mask & IN_CREATE) ||
			    (!moved_from && (event->mask & IN_MOVED_TO))) {
				// New file - if it is a directory, watch it
				char* new_file =
				    inotifytools_dirpath_from_event(event);
				if (new_file && *new_file && isdir(new_file) &&
				    !inotifytools_watch_recursively(new_file,
								    events)) {
					fprintf(stderr,
						"Couldn't watch new directory "
						"%s: %s\n",
						new_file,
						strerror(inotifytools_error()));
				}
				free(new_file);
			}  // IN_CREATE
			else if (event->mask & IN_MOVED_FROM) {
				moved_from =
				    inotifytools_dirpath_from_event(event);
				// if not watched...
				if (inotifytools_wd_from_filename(moved_from) ==
				    -1) {
					free(moved_from);
					moved_from = 0;
				}
			}  // IN_MOVED_FROM
			else if (event->mask & IN_MOVED_TO) {
				if (moved_from) {
					char* new_name =
					    inotifytools_dirpath_from_event(
						event);
					inotifytools_replace_filename(
					    moved_from, new_name);
					free(new_name);
					free(moved_from);
					moved_from = 0;
				}  // moved_from
			}
		}

	} while (!done);

	return print_info();
}

int print_info() {
	if (!inotifytools_get_stat_total(0)) {
		fprintf(stderr, "No events occurred.\n");
		return EXIT_SUCCESS;
	}

	// OK, go through the watches and print stats.
	printf("total  ");
	if ((IN_ACCESS & events) &&
	    (zero || inotifytools_get_stat_total(IN_ACCESS)))
		printf("access  ");
	if ((IN_MODIFY & events) &&
	    (zero || inotifytools_get_stat_total(IN_MODIFY)))
		printf("modify  ");
	if ((IN_ATTRIB & events) &&
	    (zero || inotifytools_get_stat_total(IN_ATTRIB)))
		printf("attrib  ");
	if ((IN_CLOSE_WRITE & events) &&
	    (zero || inotifytools_get_stat_total(IN_CLOSE_WRITE)))
		printf("close_write  ");
	if ((IN_CLOSE_NOWRITE & events) &&
	    (zero || inotifytools_get_stat_total(IN_CLOSE_NOWRITE)))
		printf("close_nowrite  ");
	if ((IN_OPEN & events) &&
	    (zero || inotifytools_get_stat_total(IN_OPEN)))
		printf("open  ");
	if ((IN_MOVED_FROM & events) &&
	    (zero || inotifytools_get_stat_total(IN_MOVED_FROM)))
		printf("moved_from  ");
	if ((IN_MOVED_TO & events) &&
	    (zero || inotifytools_get_stat_total(IN_MOVED_TO)))
		printf("moved_to  ");
	if ((IN_MOVE_SELF & events) &&
	    (zero || inotifytools_get_stat_total(IN_MOVE_SELF)))
		printf("move_self  ");
	if ((IN_CREATE & events) &&
	    (zero || inotifytools_get_stat_total(IN_CREATE)))
		printf("create  ");
	if ((IN_DELETE & events) &&
	    (zero || inotifytools_get_stat_total(IN_DELETE)))
		printf("delete  ");
	if ((IN_DELETE_SELF & events) &&
	    (zero || inotifytools_get_stat_total(IN_DELETE_SELF)))
		printf("delete_self  ");
	if ((IN_UNMOUNT & events) &&
	    (zero || inotifytools_get_stat_total(IN_UNMOUNT)))
		printf("unmount  ");

	printf("filename\n");

	struct rbtree* tree = inotifytools_wd_sorted_by_event(sort);
	RBLIST* rblist = rbopenlist(tree);
	watch* w = (watch*)rbreadlist(rblist);

	while (w) {
		if (!zero && !w->hit_total) {
			w = (watch*)rbreadlist(rblist);
			continue;
		}
		printf("%-5u  ", w->hit_total);
		if ((IN_ACCESS & events) &&
		    (zero || inotifytools_get_stat_total(IN_ACCESS)))
			printf("%-6u  ", w->hit_access);
		if ((IN_MODIFY & events) &&
		    (zero || inotifytools_get_stat_total(IN_MODIFY)))
			printf("%-6u  ", w->hit_modify);
		if ((IN_ATTRIB & events) &&
		    (zero || inotifytools_get_stat_total(IN_ATTRIB)))
			printf("%-6u  ", w->hit_attrib);
		if ((IN_CLOSE_WRITE & events) &&
		    (zero || inotifytools_get_stat_total(IN_CLOSE_WRITE)))
			printf("%-11u  ", w->hit_close_write);
		if ((IN_CLOSE_NOWRITE & events) &&
		    (zero || inotifytools_get_stat_total(IN_CLOSE_NOWRITE)))
			printf("%-13u  ", w->hit_close_nowrite);
		if ((IN_OPEN & events) &&
		    (zero || inotifytools_get_stat_total(IN_OPEN)))
			printf("%-4u  ", w->hit_open);
		if ((IN_MOVED_FROM & events) &&
		    (zero || inotifytools_get_stat_total(IN_MOVED_FROM)))
			printf("%-10u  ", w->hit_moved_from);
		if ((IN_MOVED_TO & events) &&
		    (zero || inotifytools_get_stat_total(IN_MOVED_TO)))
			printf("%-8u  ", w->hit_moved_to);
		if ((IN_MOVE_SELF & events) &&
		    (zero || inotifytools_get_stat_total(IN_MOVE_SELF)))
			printf("%-9u  ", w->hit_move_self);
		if ((IN_CREATE & events) &&
		    (zero || inotifytools_get_stat_total(IN_CREATE)))
			printf("%-6u  ", w->hit_create);
		if ((IN_DELETE & events) &&
		    (zero || inotifytools_get_stat_total(IN_DELETE)))
			printf("%-6u  ", w->hit_delete);
		if ((IN_DELETE_SELF & events) &&
		    (zero || inotifytools_get_stat_total(IN_DELETE_SELF)))
			printf("%-11u  ", w->hit_delete_self);
		if ((IN_UNMOUNT & events) &&
		    (zero || inotifytools_get_stat_total(IN_UNMOUNT)))
			printf("%-7u  ", w->hit_unmount);

		printf("%s\n", inotifytools_filename_from_watch(w));
		w = (watch*)rbreadlist(rblist);
	}
	rbcloselist(rblist);
	rbdestroy(tree);

	return EXIT_SUCCESS;
}

static bool parse_opts(int* argc,
		       char*** argv,
		       int* e,
		       long* timeout,
		       int* verbose,
		       int* z,
		       int* s,
		       int* recursive,
		       int* no_dereference,
		       char** fromfile,
		       char** exc_regex,
		       char** exc_iregex,
		       char** inc_regex,
		       char** inc_iregex,
		       int* fanotify,
		       bool* filesystem) {
	assert(argc);
	assert(argv);
	assert(e);
	assert(timeout);
	assert(verbose);
	assert(z);
	assert(s);
	assert(recursive);
	assert(fanotify);
	assert(filesystem);
	assert(no_dereference);
	assert(fromfile);
	assert(exc_regex);
	assert(exc_iregex);
	assert(inc_regex);
	assert(inc_iregex);

	// Settings for options
	int new_event;
	bool sort_set = false;

	// Short options
	static const char opt_string[] = "hrPa:d:zve:t:IFS";

	// Construct array
	static const struct option long_opts[] = {
	    {"help", no_argument, NULL, 'h'},
	    {"event", required_argument, NULL, 'e'},
	    {"timeout", required_argument, NULL, 't'},
	    {"verbose", no_argument, NULL, 'v'},
	    {"zero", no_argument, NULL, 'z'},
	    {"ascending", required_argument, NULL, 'a'},
	    {"descending", required_argument, NULL, 'd'},
	    {"recursive", no_argument, NULL, 'r'},
	    {"inotify", no_argument, NULL, 'I'},
	    {"fanotify", no_argument, NULL, 'F'},
	    {"filesystem", no_argument, NULL, 'S'},
	    {"no-dereference", no_argument, NULL, 'P'},
	    {"fromfile", required_argument, NULL, 'o'},
	    {"exclude", required_argument, NULL, 'c'},
	    {"excludei", required_argument, NULL, 'b'},
	    {"include", required_argument, NULL, 'j'},
	    {"includei", required_argument, NULL, 'k'},
	    {NULL, 0, 0, 0},
	};

	// Get first option
	char curr_opt = getopt_long(*argc, *argv, opt_string, long_opts, NULL);

	// While more options exist...
	while ((curr_opt != '?') && (curr_opt != (char)-1)) {
		switch (curr_opt) {
			// --help or -h
			case 'h':
				print_help(((*argc) > 0)
					   ? basename((*argv)[0])
					   : "<executable>");
				// Shouldn't process any further...
				return false;

			// --verbose or -v
			case 'v':
				++(*verbose);
				break;

			// --recursive or -r
			case 'r':
				++(*recursive);
				break;

			// --inotify or -I
			case 'I':
				(*fanotify) = 0;
				break;

			// --fanotify or -F
			case 'F':
				(*fanotify) = 1;
				break;

			// --filesystem or -S
			case 'S':
				(*filesystem) = true;
				(*fanotify) = 1;
				break;

			case 'P':
				++(*no_dereference);
				break;

			// --zero or -z
			case 'z':
				++(*z);
				break;

			// --exclude
			case 'c':
				(*exc_regex) = optarg;
				break;

			// --excludei
			case 'b':
				(*exc_iregex) = optarg;
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
			case 'o':
				if (*fromfile) {
					fprintf(stderr,
						"Multiple --fromfile options "
						"given.\n");
					return false;
				}
				(*fromfile) = optarg;
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
				if (new_event == -1) {
					fprintf(
					    stderr,
					    "'%s' is not a valid event!  Run "
					    "with the "
					    "'--help' option to see a list of "
					    "events.\n",
					    optarg);
					return false;
				}

				// Add the new event to the event mask
				(*e) = ((*e) | new_event);

				break;

			// --ascending or -a
			case 'a':
				assert(optarg);
				if (sort_set) {
					fprintf(stderr,
						"Please specify -a or -d once "
						"only!\n");
					return false;
				}

				if (0 == strcasecmp(optarg, "total")) {
					(*s) = 0;
				} else if (0 == strcasecmp(optarg, "move")) {
					fprintf(
					    stderr,
					    "Cannot sort by `move' event; "
					    "please use "
					    "`moved_from' or `moved_to'.\n");
					return false;
				} else if (0 == strcasecmp(optarg, "close")) {
					fprintf(stderr,
						"Cannot sort by `close' event; "
						"please use "
						"`close_write' or "
						"`close_nowrite'.\n");
					return false;
				} else {
					int event =
					    inotifytools_str_to_event(optarg);

					// If optarg was invalid, abort
					if (event == -1) {
						fprintf(stderr,
							"'%s' is not a valid "
							"key for "
							"sorting!\n",
							optarg);
						return false;
					}

					(*s) = event;
				}
				sort_set = true;
				break;

			// --descending or -d
			case 'd':
				assert(optarg);
				if (sort_set) {
					fprintf(stderr,
						"Please specify -a or -d once "
						"only!\n");
					return false;
				}

				if (0 == strcasecmp(optarg, "total")) {
					(*s) = -1;
				} else {
					int event =
					    inotifytools_str_to_event(optarg);

					// If optarg was invalid, abort
					if (event == -1) {
						fprintf(stderr,
							"'%s' is not a valid "
							"key for "
							"sorting!\n",
							optarg);
						return false;
					}

					(*s) = -event;
				}
				break;
		}

		curr_opt =
		    getopt_long(*argc, *argv, opt_string, long_opts, NULL);
	}

	(*argc) -= optind;
	*argv = &(*argv)[optind];

	if ((*s) != 0 && (*s) != -1 &&
	    !(abs(*s) & ((*e) ? (*e) : IN_ALL_EVENTS))) {
		fprintf(stderr,
			"Can't sort by an event which isn't being watched "
			"for!\n");
		return false;
	}

	if (*exc_regex && *exc_iregex) {
		fprintf(stderr,
			"--exclude and --excludei cannot both be specified.\n");
		return false;
	}
	if (*inc_regex && *inc_iregex) {
		fprintf(stderr,
			"--include and --includei cannot both be specified.\n");
		return false;
	}
	if ((*inc_regex && *exc_regex) || (*inc_regex && *exc_iregex) ||
	    (*inc_iregex && *exc_regex) || (*inc_iregex && *exc_iregex)) {
		fprintf(
		    stderr,
		    "include and exclude regexp cannot both be specified.\n");
		return false;
	}

	// If ? returned, invalid option
	return (curr_opt != '?');
}

void print_help(const char *tool_name) {
	printf("%s %s\n", tool_name, PACKAGE_VERSION);
	printf("Gather filesystem usage statistics using %s.\n", tool_name);
	printf("Usage: %s [ options ] file1 [ file2 ] [ ... ]\n", tool_name);
	printf("Options:\n");
	printf("\t-h|--help    \tShow this help text.\n");
	printf("\t-v|--verbose \tBe verbose.\n");
	printf(
	    "\t@<file>       \tExclude the specified file from being "
	    "watched.\n");
	printf(
	    "\t--fromfile <file>\n"
	    "\t\tRead files to watch from <file> or `-' for stdin.\n");
	printf(
	    "\t--exclude <pattern>\n"
	    "\t\tExclude all events on files matching the extended regular\n"
	    "\t\texpression <pattern>.\n");
	printf(
	    "\t--excludei <pattern>\n"
	    "\t\tLike --exclude but case insensitive.\n");
	printf(
	    "\t--include <pattern>\n"
	    "\t\tExclude all events on files except the ones\n"
	    "\t\tmatching the extended regular expression\n"
	    "\t\t<pattern>.\n");
	printf(
	    "\t--includei <pattern>\n"
	    "\t\tLike --include but case insensitive.\n");
	printf(
	    "\t-z|--zero\n"
	    "\t\tIn the final table of results, output rows and columns even\n"
	    "\t\tif they consist only of zeros (the default is to not output\n"
	    "\t\tthese rows and columns).\n");
	printf("\t-r|--recursive\tWatch directories recursively.\n");
	printf("\t-I|--inotify\tWatch with inotify.\n");
	printf("\t-F|--fanotify\tWatch with fanotify.\n");
	printf("\t-S|--filesystem\tWatch entire filesystem with fanotify.\n");
	printf(
	    "\t-P|--no-dereference\n"
	    "\t\tDo not follow symlinks.\n");
	printf(
	    "\t-t|--timeout <seconds>\n"
	    "\t\tListen only for specified amount of time in seconds; if\n"
	    "\t\tomitted or zero, %s will execute until receiving an\n"
	    "\t\tinterrupt signal.\n",
	    tool_name);
	printf(
	    "\t-e|--event <event1> [ -e|--event <event2> ... ]\n"
	    "\t\tListen for specific event(s).  If omitted, all events are \n"
	    "\t\tlistened for.\n");
	printf(
	    "\t-a|--ascending <event>\n"
	    "\t\tSort ascending by a particular event, or `total'.\n");
	printf(
	    "\t-d|--descending <event>\n"
	    "\t\tSort descending by a particular event, or `total'.\n\n");
	printf("Exit status:\n");
	printf("\t%d  -  Exited normally.\n", EXIT_SUCCESS);
	printf("\t%d  -  Some error occurred.\n\n", EXIT_FAILURE);
	printf("Events:\n");
	print_event_descriptions();
}
