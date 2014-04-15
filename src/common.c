#include "../config.h"
#include "common.h"

#include <sys/stat.h>
#include <sys/types.h>

#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <inotifytools/inotifytools.h>

#define MAXLEN 4096
#define LIST_CHUNK 1024

#define resize_if_necessary(count,len,ptr) \
	if ( count >= len - 1 ) { \
		len += LIST_CHUNK; \
		ptr =(char const **)realloc(ptr, sizeof(char *)*len); \
	}

void print_event_descriptions() {
	printf("\taccess\t\tfile or directory contents were read\n");
	printf("\tmodify\t\tfile or directory contents were written\n");
	printf("\tattrib\t\tfile or directory attributes changed\n");
	printf("\tclose_write\tfile or directory closed, after being opened in\n"
	       "\t           \twriteable mode\n");
	printf("\tclose_nowrite\tfile or directory closed, after being opened in\n"
	       "\t           \tread-only mode\n");
	printf("\tclose\t\tfile or directory closed, regardless of read/write "
	       "mode\n");
	printf("\topen\t\tfile or directory opened\n");
	printf("\tmoved_to\tfile or directory moved to watched directory\n");
	printf("\tmoved_from\tfile or directory moved from watched directory\n");
	printf("\tmove\t\tfile or directory moved to or from watched directory\n");
	printf("\tmove_self\t\tA watched file or directory was moved.\n");
	printf("\tcreate\t\tfile or directory created within watched directory\n");
	printf("\tdelete\t\tfile or directory deleted within watched directory\n");
	printf("\tdelete_self\tfile or directory was deleted\n");
	printf("\tunmount\t\tfile system containing file or directory unmounted\n");
}

int isdir( char const * path ) {
	static struct stat64 my_stat;

	if ( -1 == lstat64( path, &my_stat ) ) {
		if (errno == ENOENT) return 0;
		fprintf(stderr, "Stat failed on %s: %s\n", path, strerror(errno));
		return 0;
	}

	return S_ISDIR( my_stat.st_mode ) && !S_ISLNK( my_stat.st_mode );
}

FileList construct_path_list( int argc, char ** argv, char const * filename ) {
	FileList list;
	list.watch_files = 0;
	list.exclude_files = 0;
	FILE * file = 0;

	if (!filename) {
	}
	else if (!strcmp(filename,"-")) {
		file = stdin;
	}
	else {
		file = fopen( filename, "r" );
	}

	int watch_len = LIST_CHUNK;
	int exclude_len = LIST_CHUNK;
	int watch_count = 0;
	int exclude_count = 0;
	list.watch_files = (char const **)malloc(sizeof(char *)*watch_len);
	list.exclude_files = (char const **)malloc(sizeof(char *)*exclude_len);

	char name[MAXLEN];
	while ( file && fgets(name, MAXLEN, file) ) {
		if ( name[strlen(name)-1] == '\n') name[strlen(name)-1] = 0;
		if ( strlen(name) == 0 ) continue;
		if ( '@' == name[0] && strlen(name) == 1 ) continue;
		if ( '@' == name[0] ) {
			resize_if_necessary(exclude_count, exclude_len, list.exclude_files);
			list.exclude_files[exclude_count++] = strdup(&name[1]);
		}
		else {
			resize_if_necessary(watch_count, watch_len, list.watch_files);
			list.watch_files[watch_count++] = strdup(name);
		}
	}
	if ( file && file != stdin) fclose(file);

	for ( int i = 0; i < argc; ++i ) {
		if ( strlen(argv[i]) == 0 ) continue;
		if ( '@' == argv[i][0] && strlen(argv[i]) == 1 ) continue;
		if ( '@' == argv[i][0] ) {
			resize_if_necessary(exclude_count, exclude_len, list.exclude_files);
			list.exclude_files[exclude_count++] = &argv[i][1];
		}
		else {
			resize_if_necessary(watch_count, watch_len, list.watch_files);
			list.watch_files[watch_count++] = argv[i];
		}
	}
	list.exclude_files[exclude_count] = 0;
	list.watch_files[watch_count] = 0;
	return list;
}

void _niceassert( long cond, int line, char const * file, char const * condstr,
                  char const * mesg ) {
	if ( cond ) return;

	if ( mesg ) {
		fprintf(stderr, "%s:%d assertion ( %s ) failed: %s\n", file, line,
		        condstr, mesg );
	}
	else {
		fprintf(stderr, "%s:%d assertion ( %s ) failed.\n", file, line, condstr);
	}
}

void warn_inotify_init_error()
{
	int error = inotifytools_error();
	fprintf(stderr, "Couldn't initialize inotify: %s\n", strerror(error));
	if (error == EMFILE) {
		fprintf(stderr, "Try increasing the value of /proc/sys/fs/inotify/max_user_instances\n");
	}
}

bool is_timeout_option_valid(long int *timeout, char *optarg)
{
	if ((optarg == NULL) || (*optarg == '\0')) {
		fprintf(stderr, "The provided value is not a valid timeout value.\n"
				"Please specify a long int value.\n");
		return false;
	}

	char *timeout_end = NULL;
	errno = 0;
	*timeout = strtol(optarg, &timeout_end, 10);

	const int err = errno;
	if (err != 0) {
		if (err == ERANGE) {
			// Figure out on which side it overflows.
			if (*timeout == LONG_MAX) {
				fprintf(stderr, "The timeout value you provided is "
						"not in the representable range "
						"(higher than LONG_MAX).\n");
			} else {
				fprintf(stderr, "The timeout value you provided is "
						"not in the representable range "
						"(lower than LONG_MIN).\n");
			}

		} else {
			fprintf(stderr, "Something went wrong with the timeout "
					"value you provided.\n");
			fprintf(stderr, "%s\n", strerror(err));
		}
		return false;
	}

	if (*timeout_end != '\0') {
		fprintf(stderr, "'%s' is not a valid timeout value.\n"
				"Please specify a long int value.\n", optarg);
		return false;
	}

	return true;
}
