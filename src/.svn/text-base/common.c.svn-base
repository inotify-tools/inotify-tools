// kate: replace-tabs off; space-indent off;

#include "../config.h"

#include <stdio.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>
#include <stdlib.h>

#include "common.h"

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
