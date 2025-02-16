#include "common.h"
#include "../config.h"

#include <sys/stat.h>
#include <sys/types.h>

#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <inotifytools/inotifytools.h>

#define MAXLEN 4096
#define LIST_CHUNK 1024

void print_event_descriptions() {
	printf(
	    "\taccess\t\tfile or directory contents were read\n"
	    "\tmodify\t\tfile or directory contents were written\n"
	    "\tattrib\t\tfile or directory attributes changed\n"
	    "\tclose_write\tfile or directory closed, after being opened in\n"
	    "\t           \twritable mode\n"
	    "\tclose_nowrite\tfile or directory closed, after being opened in\n"
	    "\t           \tread-only mode\n"
	    "\tclose\t\tfile or directory closed, regardless of read/write "
	    "mode\n"
	    "\topen\t\tfile or directory opened\n"
	    "\tmoved_to\tfile or directory moved to watched directory\n"
	    "\tmoved_from\tfile or directory moved from watched directory\n"
	    "\tmove\t\tfile or directory moved to or from watched directory\n"
	    "\tmove_self\t\tA watched file or directory was moved.\n"
	    "\tcreate\t\tfile or directory created within watched directory\n"
	    "\tdelete\t\tfile or directory deleted within watched directory\n"
	    "\tdelete_self\tfile or directory was deleted\n"
	    "\tunmount\t\tfile system containing file or directory "
	    "unmounted\n");
}

int isdir(char const* path) {
	static struct stat my_stat;

	if (-1 == lstat(path, &my_stat)) {
		if (errno == ENOENT)
			return 0;
		fprintf(stderr, "Stat failed on %s: %s\n", path,
			strerror(errno));
		return 0;
	}

	return S_ISDIR(my_stat.st_mode) && !S_ISLNK(my_stat.st_mode);
}

FileList::FileList(int argc, char** argv)
    : watch_files_(0), exclude_files_(0), argc_(argc), argv_(argv) {}

FileList::~FileList() {
	for (int i = 0; watch_files_[i]; ++i) {
		free((void*)watch_files_[i]);
	}

	free(watch_files_);

	for (int i = 0; exclude_files_[i]; ++i) {
		free((void*)exclude_files_[i]);
	}

	free(exclude_files_);
}

struct file {
	FILE* file_;
	bool is_stdin;

	FILE* open(const char* filename) {
		file_ = fopen(filename, "r");
		return file_;
	}

	FILE* get() {
		if (is_stdin)
			return stdin;

		return file_;
	}

	file() : file_(nullptr), is_stdin(false) {}

	~file() {
		if (file_)
			fclose(file_);
	}
};

void construct_path_list(int argc,
			 char** argv,
			 char const* filename,
			 FileList* list) {
	list->watch_files_ = 0;
	list->exclude_files_ = 0;
	file file;

	if (filename) {
		if (filename[0] == '-' && !filename[1])
			file.is_stdin = true;
		else if (!file.open(filename)) {
			fprintf(stderr, "Couldn't open %s: %s\n", filename,
				strerror(errno));
                        return;
                }
	}

	size_t watch_count = 0;
	size_t watch_allocated = LIST_CHUNK;
	size_t exclude_count = 0;
	size_t exclude_allocated = LIST_CHUNK;
	list->watch_files_ = (char const**)malloc(sizeof(char*) * watch_allocated);
	if (!list->watch_files_)
		return;

	list->exclude_files_ = (char const**)malloc(sizeof(char*) * exclude_allocated);
	if (!list->exclude_files_)
		return;

	char name[MAXLEN];
	while (file.get() && fgets(name, MAXLEN, file.get())) {
		const size_t str_len = strlen(name);
		if (name[str_len - 1] == '\n')
			name[str_len - 1] = 0;

		if (!str_len || ('@' == name[0] && str_len == 1))
			continue;

		if ('@' == name[0]) {
			if (exclude_count == exclude_allocated - 1) {
				exclude_allocated *= 2;
				auto mem = (char const**) realloc (list->exclude_files_, sizeof(char*) * exclude_allocated);
				if (!mem) {
					list->watch_files_[watch_count] = NULL;
					list->exclude_files_[exclude_count] = NULL;
					return;
				}
				list->exclude_files_ = mem;
			}
			list->exclude_files_[exclude_count++] =
			    strdup(&name[1]);
			continue;
		}

		if (watch_count == watch_allocated - 1) {
			watch_allocated *= 2;
			auto mem = (char const**) realloc (list->watch_files_, sizeof(char*) * watch_allocated);
			if (!mem) {
				list->watch_files_[watch_count] = NULL;
				list->exclude_files_[exclude_count] = NULL;
				return;
			}
			list->watch_files_ = mem;
		}
		list->watch_files_[watch_count++] = strdup(name);
	}

	for (int i = 0; i < argc; ++i) {
		const size_t str_len = strlen(argv[i]);
		if (!str_len || ('@' == argv[i][0] && str_len == 1))
			continue;

		if ('@' == argv[i][0]) {
			if (exclude_count == exclude_allocated - 1) {
				exclude_allocated *= 2;
				auto mem = (char const**) realloc (list->exclude_files_, sizeof(char*) * exclude_allocated);
				if (!mem) {
					list->watch_files_[watch_count] = NULL;
					list->exclude_files_[exclude_count] = NULL;
					return;
				}
				list->exclude_files_ = mem;
			}
			list->exclude_files_[exclude_count++] =
			    strdup(&argv[i][1]);
			continue;
		}

		if (watch_count == watch_allocated - 1) {
			watch_allocated *= 2;
			auto mem = (char const**) realloc (list->watch_files_, sizeof(char*) * watch_allocated);
			if (!mem) {
				list->watch_files_[watch_count] = NULL;
				list->exclude_files_[exclude_count] = NULL;
				return;
			}
			list->watch_files_ = mem;
		}
		list->watch_files_[watch_count++] = strdup(argv[i]);
	}

	list->exclude_files_[exclude_count] = 0;
	list->watch_files_[watch_count] = 0;
}

void warn_inotify_init_error(int fanotify) {
	const char* backend = fanotify ? "fanotify" : "inotify";
	const char* resource = fanotify ? "groups" : "instances";
	int error = inotifytools_error();

	fprintf(stderr, "Couldn't initialize %s: %s\n", backend,
		strerror(error));
	if (error == EMFILE) {
		fprintf(stderr,
			"Try increasing the value of "
			"/proc/sys/fs/%s/max_user_%s\n",
			backend, resource);
	}
	if (fanotify && error == EINVAL) {
		fprintf(stderr,
			"fanotify support for reporting the events with "
			"file names was added in kernel v5.9.\n");
	}
	if (fanotify && error == EPERM) {
		fprintf(stderr, "fanotify watch requires admin privileges\n");
	}
}

bool is_timeout_option_valid(long* timeout, char* o) {
	if ((o == NULL) || (*o == '\0')) {
		fprintf(stderr,
			"The provided value is not a valid timeout value.\n"
			"Please specify a long int value.\n");
		return false;
	}

	char* timeout_end = NULL;
	errno = 0;
	*timeout = strtol(o, &timeout_end, 10);

	if (errno) {
		fprintf(stderr,
			"Something went wrong with the timeout "
			"value you provided.\n");
		fprintf(stderr, "%s\n", strerror(errno));
		return false;
	}

	if (*timeout_end != '\0') {
		fprintf(stderr,
			"'%s' is not a valid timeout value.\n"
			"Please specify a long int value.\n",
			o);
		return false;
	}

	return true;
}
