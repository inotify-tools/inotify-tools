// kate: replace-tabs off; space-indent off;

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
