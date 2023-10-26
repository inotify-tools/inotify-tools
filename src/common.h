#ifndef COMMON_H
#define COMMON_H

#include <stdbool.h>

#define BLOCKING_TIMEOUT 0

#ifndef EXIT_SUCCESS
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#endif
#define EXIT_TIMEOUT 2

void print_event_descriptions();
int isdir(char const *path);

struct FileList {
	char const** watch_files_;
	char const** exclude_files_;
	int argc_;
	char** argv_;

	FileList(int argc, char** argv);
	~FileList();
};

void construct_path_list(int argc,
			 char** argv,
			 char const* filename,
			 FileList* list);

void warn_inotify_init_error(int fanotify);

bool is_timeout_option_valid(long* timeout, char* o);

#endif
