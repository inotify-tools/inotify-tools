#ifndef COMMON_H
#define COMMON_H

#ifndef EXIT_SUCCESS
    #define EXIT_SUCCESS 0
    #define EXIT_FAILURE 1
#endif
#define EXIT_TIMEOUT 2

void print_event_descriptions();
int isdir( char const * path );

typedef struct {
	char const ** watch_files;
	char const ** exclude_files;
} FileList;
FileList construct_path_list( int argc, char ** argv, char const * filename );

#define niceassert(cond,mesg) _niceassert((long)cond, __LINE__, __FILE__, \
                                          #cond, mesg)

void _niceassert( long cond, int line, char const * file, char const * condstr,
                  char const * mesg );

#endif
