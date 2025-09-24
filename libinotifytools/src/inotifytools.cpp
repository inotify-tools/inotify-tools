// kate: replace-tabs off; space-indent off;

/**
 * @mainpage libinotifytools
 *
 * libinotifytools is a small C library to simplify the use of Linux's inotify
 * interface.
 *
 * @link inotifytools/inotifytools.h Documentation for the library's public
 * interface.@endlink
 *
 * @link todo.html TODO list.@endlink
 */

#include "inotifytools/inotifytools.h"
#include "../../config.h"
#include "inotifytools_p.h"
#include "stats.h"

#include <dirent.h>
#include <errno.h>
#include <limits.h>
#include <regex.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/ioctl.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#include "inotifytools/inotify.h"

#ifdef __FreeBSD__
struct fanotify_event_fid;

#define FAN_EVENT_INFO_TYPE_FID 1
#define FAN_EVENT_INFO_TYPE_DFID_NAME 2
#define FAN_EVENT_INFO_TYPE_DFID 3

#elif !defined __ANDROID__
// Linux only
#define LINUX_FANOTIFY

#include <fcntl.h>
#include <sys/vfs.h>
#include "inotifytools/fanotify.h"

#ifndef __GLIBC__
#define val __val
#define __kernel_fsid_t fsid_t
#endif

struct fanotify_event_info_fid_wo_handle {
	struct fanotify_event_info_header hdr;
	__kernel_fsid_t fsid;
};

struct fanotify_event_fid {
	struct fanotify_event_info_fid_wo_handle info;
	struct file_handle handle;
};

#ifndef AT_HANDLE_FID
#define AT_HANDLE_FID	AT_REMOVEDIR
#endif

// from include/uapi/linux/magic.h
#ifndef BTRFS_SUPER_MAGIC
#define BTRFS_SUPER_MAGIC 0x9123683E
#endif

// from include/linux/exportfs.h
#define FILEID_BTRFS_WITHOUT_PARENT 0x4d
#endif

/**
 * @file inotifytools/inotifytools.h
 * @brief inotifytools library public interface.
 * @author Rohan McGovern, \<rohan@mcgovern.id.au\>
 *
 * This library provides a thin layer on top of the basic inotify interface.
 * The primary use is to easily set up watches on files, potentially many files
 * at once, and read events without having to deal with low-level I/O.  There
 * are also several utility functions for inotify-related string formatting.
 *
 * To use this library, you must \c \#include the following headers accordingly:
 * \li \c \<inotifytools/inotifytools.h\> - to use any functions declared in
 *     this file.
 * \li \c \<inotifytools/inotify.h\> - to have the \c inotify_event type defined
 *     and the numeric IN_* event constants defined.   If \c \<sys/inotify.h\>
 *     was present on your system at compile time, this header simply includes
 *     that.  Otherwise it includes \c \<inotifytools/inotify-nosys.h\>.
 *
 * @section example Example
 * This very simple program recursively watches the entire directory tree
 * under its working directory for events, then prints them out with a
 * timestamp.
 * @include example.c
 *
 * @section events Events
 *
 * @note This section comes almost verbatim from the inotify(7) man page.
 *
 * @warning The information here applies to inotify in Linux 2.6.17.  Older
 *          versions of Linux may not support all the events described here.
 *
 * The following numeric events can be specified to functions in inotifytools,
 * and may be present in events returned through inotifytools:
 *
 * \li \c IN_ACCESS     -     File was accessed (read) \a *
 * \li \c IN_ATTRIB     -     Metadata changed (permissions, timestamps,
 *                            extended attributes, etc.) \a *
 * \li \c IN_CLOSE_WRITE -    File opened for writing was closed \a *
 * \li \c IN_CLOSE_NOWRITE -   File not opened for writing was closed \a *
 * \li \c IN_CREATE       -   File/directory created in watched directory \a *
 * \li \c IN_DELETE       -   File/directory deleted from watched directory \a *
 * \li \c IN_DELETE_SELF  -   Watched file/directory was itself deleted
 * \li \c IN_MODIFY       -   File was modified \a *
 * \li \c IN_MOVE_SELF    -   Watched file/directory was itself moved
 * \li \c IN_MOVED_FROM   -   File moved out of watched directory \a *
 * \li \c IN_MOVED_TO     -   File moved into watched directory \a *
 * \li \c IN_OPEN         -   File was opened \a *
 *
 * When monitoring a directory, the events marked with an asterisk \a * above
 * can  occur  for files  in the directory, in which case the name field in the
 * returned inotify_event structure identifies the name of the file within the
 * directory.
 *
 * The IN_ALL_EVENTS macro is defined as a bit mask of all of the above events.
 *
 * Two additional convenience macros are IN_MOVE, which equates to
 * IN_MOVED_FROM|IN_MOVED_TO, and IN_CLOSE which equates to
 * IN_CLOSE_WRITE|IN_CLOSE_NOWRITE.
 *
 * The following bitmasks can also be provided when creating a new watch:
 *
 * \li \c IN_DONT_FOLLOW  - Don't dereference pathname if it is a symbolic link
 * \li \c IN_MASK_ADD    -  Add (OR) events to watch mask for this pathname if
 *                          it already exists (instead of replacing mask)
 * \li \c IN_ONESHOT    -   Monitor pathname for one event, then remove from
 *                          watch list
 * \li \c IN_ONLYDIR    -   Only watch pathname if it is a directory
 *
 * The following bitmasks may occur in events generated by a watch:
 *
 * \li \c IN_IGNORED   -   Watch was removed explicitly
 *                        (inotifytools_remove_watch_*) or automatically (file
 *                        was deleted, or file system was unmounted)
 * \li \c IN_ISDIR   -     Subject of this event is a directory
 * \li \c IN_Q_OVERFLOW  - Event queue overflowed (wd is -1 for this event)
 * \li \c IN_UNMOUNT    -  File system containing watched object was unmounted
 *
 * @section TODO TODO list
 *
 * @todo Improve wd/filename mapping.  Currently there is no explicit code for
 *       handling different filenames mapping to the same inode (and hence, wd).
 *       gamin's approach sounds good: let the user watch an inode using several
 *       different filenames, and when an event occurs on the inode, generate an
 *       event for each filename.
 */

#define MAX_EVENTS 4096
#define INOTIFY_PROCDIR "/proc/sys/fs/inotify/"
#define WATCHES_SIZE_PATH INOTIFY_PROCDIR "max_user_watches"
#define QUEUE_SIZE_PATH INOTIFY_PROCDIR "max_queued_watches"
#define INSTANCES_PATH INOTIFY_PROCDIR "max_user_instances"

static int inotify_fd = -1;

static int recursive_watch = 0;
int collect_stats = 0;

struct rbtree* tree_wd = 0;
struct rbtree* tree_fid = 0;
struct rbtree* tree_filename = 0;
static int error = 0;
int initialized = 0;
int verbosity = 0;
int fanotify_mode = 0;
int fanotify_mark_type = 0;
int at_handle_fid = 0;
static pid_t self_pid = 0;

struct str {
	char* c_str_ = 0;
	int size_ = 0;
	int capacity_ = 0;

	bool empty() { return !size_; }

	void clear() {
		if (c_str_) {
			c_str_[0] = 0;
			size_ = 0;
		}
	}

	void set_size(int size) {
		size_ = size;
		if (size > capacity_)
			capacity_ = size;
	}

	~str() { free(c_str_); }
};

static str timefmt;
static regex_t* regex = 0;
/* 0: --exclude[i], 1: --include[i] */
static int invert_regexp = 0;

static int isdir(char const* path);
void record_stats(struct inotify_event const* event);
int onestr_to_event(char const* event);

#define nasprintf(...) niceassert(-1 != asprintf(__VA_ARGS__), "out of memory")

/**
 * @internal
 * Assert that a condition evaluates to true, and optionally output a message
 * if the assertion fails.
 *
 * You should use the niceassert() preprocessor macro instead.
 *
 * @param  cond  If 0, assertion fails, otherwise assertion succeeds.
 *
 * @param  line  Line number of source code where assertion is made.
 *
 * @param  file  Name of source file where assertion is made.
 *
 * @param  condstr  Stringified assertion expression.
 *
 * @param  mesg  A human-readable error message shown if assertion fails.
 */
long _niceassert(long cond,
		 int line,
		 char const* file,
		 char const* condstr,
		 char const* mesg) {
	if (cond)
		return cond;

	if (mesg) {
		fprintf(stderr, "%s:%d assertion ( %s ) failed: %s\n", file,
			line, condstr, mesg);
	} else {
		fprintf(stderr, "%s:%d assertion ( %s ) failed.\n", file, line,
			condstr);
	}

	return cond;
}

static void charcat(char* s, const char c) {
	size_t l = strlen(s);
	s[l] = c;
	s[++l] = 0;
}

/**
 * @internal
 */
static int read_num_from_file(const char* filename, int* num) {
	FILE* file = fopen(filename, "r");
	if (!file) {
		error = errno;
		return 0;
	}

	if (EOF == fscanf(file, "%d", num)) {
		error = errno;
		const int fclose_ret = fclose(file);
		niceassert(!fclose_ret, 0);
		return 0;
	}

	const int fclose_ret = fclose(file);
	niceassert(!fclose_ret, 0);

	return 1;
}

static int wd_compare(const char* d1, const char* d2, const void* config) {
	if (!d1 || !d2)
		return d1 - d2;
	return ((watch*)d1)->wd - ((watch*)d2)->wd;
}

static int fid_compare(const char* d1, const char* d2, const void* config) {
#ifdef LINUX_FANOTIFY
	if (!d1 || !d2)
		return d1 - d2;
	watch* w1 = (watch*)d1;
	watch* w2 = (watch*)d2;
	int n1, n2;
	n1 = w1->fid->info.hdr.len;
	n2 = w2->fid->info.hdr.len;
	if (n1 != n2)
		return n1 - n2;
	return memcmp(w1->fid, w2->fid, n1);
#else
	return d1 - d2;
#endif
}

static int filename_compare(const char* d1,
			    const char* d2,
			    const void* config) {
	if (!d1 || !d2)
		return d1 - d2;
	return strcmp(((watch*)d1)->filename, ((watch*)d2)->filename);
}

/**
 * @internal
 */
watch* watch_from_wd(int wd) {
	watch w;
	w.wd = wd;
	return (watch*)rbfind(&w, tree_wd);
}

/**
 * @internal
 */
watch* watch_from_fid(struct fanotify_event_fid* fid) {
	watch w;
	w.fid = fid;
	return (watch*)rbfind(&w, tree_fid);
}

/**
 * @internal
 */
watch* watch_from_filename(char const* filename) {
	watch w;
	w.filename = (char*)filename;
	return (watch*)rbfind(&w, tree_filename);
}

/**
 * Initialise inotify.
 * With @fanotify non-zero, initialize fanotify filesystem watch.
 *
 * You must call this function before using any function which adds or removes
 * watches or attempts to access any information about watches.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().
 */
int inotifytools_init(int fanotify, int watch_filesystem, int verbose) {
	if (initialized)
		return 1;

	error = 0;
	verbosity = verbose;
	// Try to initialise inotify/fanotify
	if (fanotify) {
#ifdef LINUX_FANOTIFY
		self_pid = getpid();
		fanotify_mode = 1;
		fanotify_mark_type =
		    watch_filesystem ? FAN_MARK_FILESYSTEM : FAN_MARK_INODE;
		at_handle_fid =
		    watch_filesystem ? 0 : AT_HANDLE_FID;
		inotify_fd =
		    fanotify_init(FAN_REPORT_FID | FAN_REPORT_DFID_NAME, 0);
#endif
	} else {
		fanotify_mode = 0;
		inotify_fd = inotify_init();
	}
	if (inotify_fd < 0) {
		error = errno;
		return 0;
	}

	collect_stats = 0;
	initialized = 1;
	tree_wd = rbinit(wd_compare, 0);
	tree_fid = rbinit(fid_compare, 0);
	tree_filename = rbinit(filename_compare, 0);
	timefmt.clear();

	return 1;
}

int inotifytools_initialize() {
	return inotifytools_init(0, 0, 0);
}

/**
 * @internal
 */
void destroy_watch(watch* w) {
	if (w->filename)
		free(w->filename);
	if (w->fid)
		free(w->fid);
	if (w->dirf)
		close(w->dirf);
	free(w);
}

/**
 * @internal
 */
void cleanup_tree(const void* nodep,
		  const VISIT which,
		  const int depth,
		  void* arg) {
	if (which != endorder && which != leaf)
		return;
	watch* w = (watch*)nodep;
	destroy_watch(w);
}

/**
 * Close inotify and free the memory used by inotifytools.
 *
 * If you call this function, you must call inotifytools_initialize()
 * again before any other functions can be used.
 */
void inotifytools_cleanup() {
	if (!initialized)
		return;

	initialized = 0;
	close(inotify_fd);
	collect_stats = 0;
	error = 0;
	timefmt.clear();

	if (regex) {
		regfree(regex);
		free(regex);
		regex = 0;
	}

	rbwalk(tree_wd, cleanup_tree, 0);
	rbdestroy(tree_wd);
	rbdestroy(tree_fid);
	rbdestroy(tree_filename);
	tree_wd = 0;
	tree_fid = 0;
	tree_filename = 0;
}

/**
 * @internal
 */
struct replace_filename_data {
	char const* old_name;
	char const* new_name;
	size_t old_len;
};

/**
 * @internal
 */
static void replace_filename_impl(const void* nodep,
				  const VISIT which,
				  const int depth,
				  const struct replace_filename_data* data) {
	if (which != endorder && which != leaf)
		return;
	watch* w = (watch*)nodep;
	char* name;
	if (0 == strncmp(data->old_name, w->filename, data->old_len)) {
		nasprintf(&name, "%s%s", data->new_name,
			  &(w->filename[data->old_len]));
		if (!strcmp(w->filename, data->new_name)) {
			free(name);
		} else {
			rbdelete(w, tree_filename);
			free(w->filename);
			w->filename = name;
			rbsearch(w, tree_filename);
		}
	}
}

/**
 * @internal
 */
static void replace_filename(const void* nodep,
			     const VISIT which,
			     const int depth,
			     void* data) {
	replace_filename_impl(nodep, which, depth,
			      (const struct replace_filename_data*)data);
}

/**
 * @internal
 */
static void get_num(const void* nodep,
		    const VISIT which,
		    const int depth,
		    void* arg) {
	if (which != endorder && which != leaf)
		return;
	++(*((int*)arg));
}

/**
 * Convert character separated events from string form to integer form
 * (as in inotify.h).
 *
 * @param    event    a sequence of events in string form as defined in
 *                    inotify.h without leading IN_ prefix (e.g., MODIFY,
 *                    ATTRIB), separated by the @a sep character.  Case
 *                    insensitive.  Can be a single event.
 *                    Can be empty or NULL.  See section \ref events.
 *
 * @param    sep      Character used to separate events.  @a sep must not be
 *                    a character in a-z, A-Z, or _.
 *
 * @return            integer representing the mask specified by @a event, or 0
 *                    if any string in @a event is empty or NULL, or -1 if
 *                    any string in @a event does not match any event or
 *                    @a sep is invalid.
 *
 * @section example Example
 * @code
 * char * eventstr = "MODIFY:CLOSE:CREATE";
 * int eventnum = inotifytools_str_to_event_sep( eventstr, ':' );
 * if ( eventnum == IN_MODIFY | IN_CLOSE | IN_CREATE ) {
 *    printf( "This code always gets executed!\n" );
 * }
 * @endcode
 */
int inotifytools_str_to_event_sep(char const* event, char sep) {
	if (strchr("_"
		   "abcdefghijklmnopqrstuvwxyz"
		   "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
		   sep)) {
		return -1;
	}

	int ret, len;
	char *event1, *event2;
	static const size_t eventstr_size = 4096;
	char eventstr[eventstr_size];
	ret = 0;

	if (!event || !event[0])
		return 0;

	event1 = (char*)event;
	event2 = strchr(event1, sep);
	while (event1 && event1[0]) {
		if (event2) {
			len = event2 - event1;
			niceassert(len < eventstr_size,
				   "malformed event string (very long)");
		} else {
			len = strlen(event1);
		}
		if (len > eventstr_size - 1)
			len = eventstr_size - 1;

		strncpy(eventstr, event1, len);

		eventstr[len] = 0;

		int ret1 = onestr_to_event(eventstr);
		if (0 == ret1 || -1 == ret1) {
			ret = ret1;
			break;
		}
		ret |= ret1;

		event1 = event2;
		if (event1 && event1[0]) {
			// jump over 'sep' character
			++event1;
			// if last character was 'sep'...
			if (!event1[0])
				return 0;
			event2 = strchr(event1, sep);
		}
	}

	return ret;
}

/**
 * Convert comma-separated events from string form to integer form
 * (as in inotify.h).
 *
 * @param    event    a sequence of events in string form as defined in
 *                    inotify.h without leading IN_ prefix (e.g., MODIFY,
 *                    ATTRIB), comma-separated.  Case
 *                    insensitive.  Can be a single event.
 *                    Can be empty or NULL.  See section \ref events.
 *
 * @return            integer representing the mask specified by @a event, or 0
 *                    if any string in @a event is empty or NULL, or -1 if
 *                    any string in @a event does not match any event.
 *
 * @section example Example
 * @code
 * char * eventstr = "MODIFY,CLOSE,CREATE";
 * int eventnum = inotifytools_str_to_event( eventstr );
 * if ( eventnum == IN_MODIFY | IN_CLOSE | IN_CREATE ) {
 *    printf( "This code always gets executed!\n" );
 * }
 * @endcode
 */
int inotifytools_str_to_event(char const* event) {
	return inotifytools_str_to_event_sep(event, ',');
}

/**
 * @internal
 * Convert a single event from string form to integer form (as in inotify.h).
 *
 * @param    event    event in string form as defined in inotify.h without
 *                    leading IN_ prefix (e.g., MODIFY, ATTRIB).  Case
 *                    insensitive.  Can be empty or NULL.
 * @return            integer representing the mask specified by 'event', or 0
 *                    if @a event is empty or NULL, or -1 if string does not
 *                    match any event.
 */
int onestr_to_event(char const* event) {
	static int ret;
	ret = -1;

	if (!event || !event[0])
		ret = 0;
	else if (0 == strcasecmp(event, "ACCESS"))
		ret = IN_ACCESS;
	else if (0 == strcasecmp(event, "MODIFY"))
		ret = IN_MODIFY;
	else if (0 == strcasecmp(event, "ATTRIB"))
		ret = IN_ATTRIB;
	else if (0 == strcasecmp(event, "CLOSE_WRITE"))
		ret = IN_CLOSE_WRITE;
	else if (0 == strcasecmp(event, "CLOSE_NOWRITE"))
		ret = IN_CLOSE_NOWRITE;
	else if (0 == strcasecmp(event, "OPEN"))
		ret = IN_OPEN;
	else if (0 == strcasecmp(event, "MOVED_FROM"))
		ret = IN_MOVED_FROM;
	else if (0 == strcasecmp(event, "MOVED_TO"))
		ret = IN_MOVED_TO;
	else if (0 == strcasecmp(event, "CREATE"))
		ret = IN_CREATE;
	else if (0 == strcasecmp(event, "DELETE"))
		ret = IN_DELETE;
	else if (0 == strcasecmp(event, "DELETE_SELF"))
		ret = IN_DELETE_SELF;
	else if (0 == strcasecmp(event, "UNMOUNT"))
		ret = IN_UNMOUNT;
	else if (0 == strcasecmp(event, "Q_OVERFLOW"))
		ret = IN_Q_OVERFLOW;
	else if (0 == strcasecmp(event, "IGNORED"))
		ret = IN_IGNORED;
	else if (0 == strcasecmp(event, "CLOSE"))
		ret = IN_CLOSE;
	else if (0 == strcasecmp(event, "MOVE_SELF"))
		ret = IN_MOVE_SELF;
	else if (0 == strcasecmp(event, "MOVE"))
		ret = IN_MOVE;
	else if (0 == strcasecmp(event, "ISDIR"))
		ret = IN_ISDIR;
	else if (0 == strcasecmp(event, "ONESHOT"))
		ret = IN_ONESHOT;
	else if (0 == strcasecmp(event, "ALL_EVENTS"))
		ret = IN_ALL_EVENTS;

	return ret;
}

/**
 * Convert event from integer form to string form (as in inotify.h).
 *
 * The returned string is from static storage; subsequent calls to this function
 * or inotifytools_event_to_str_sep() will overwrite it.  Don't free() it and
 * make a copy if you want to keep it.
 *
 * @param    events   OR'd event(s) in integer form as defined in inotify.h.
 *                    See section \ref events.
 *
 * @return            comma-separated string representing the event(s), in no
 *                    particular order
 *
 * @section example Example
 * @code
 * int eventnum == IN_MODIFY | IN_CLOSE | IN_CREATE;
 * char * eventstr = inotifytools_event_to_str( eventnum );
 * printf( "%s\n", eventstr );
 * // outputs something like MODIFY,CLOSE,CREATE but order not guaranteed.
 * @endcode
 */
char* inotifytools_event_to_str(int events) {
	return inotifytools_event_to_str_sep(events, ',');
}

/**
 * Convert event from integer form to string form (as in inotify.h).
 *
 * The returned string is from static storage; subsequent calls to this function
 * or inotifytools_event_to_str() will overwrite it.  Don't free() it and
 * make a copy if you want to keep it.
 *
 * @param    events   OR'd event(s) in integer form as defined in inotify.h
 *
 * @param    sep      character used to separate events
 *
 * @return            @a sep separated string representing the event(s), in no
 *                    particular order.  If the integer is not made of OR'ed
 *                    inotify events, the string returned will be a hexadecimal
 *                    representation of the integer.
 *
 * @section example Example
 * @code
 * int eventnum == IN_MODIFY | IN_CLOSE | IN_CREATE;
 * char * eventstr = inotifytools_event_to_str_sep( eventnum, '-' );
 * printf( "%s\n", eventstr );
 * // outputs something like MODIFY-CLOSE-CREATE but order not guaranteed.
 * @endcode
 */
char* inotifytools_event_to_str_sep(int events, char sep) {
	static char ret[1024];
	ret[0] = '\0';
	ret[1] = '\0';

	if (IN_ACCESS & events) {
		charcat(ret, sep);
		strncat(ret, "ACCESS", 7);
	}
	if (IN_MODIFY & events) {
		charcat(ret, sep);
		strncat(ret, "MODIFY", 7);
	}
	if (IN_ATTRIB & events) {
		charcat(ret, sep);
		strncat(ret, "ATTRIB", 7);
	}
	if (IN_CLOSE_WRITE & events) {
		charcat(ret, sep);
		strncat(ret, "CLOSE_WRITE", 12);
	}
	if (IN_CLOSE_NOWRITE & events) {
		charcat(ret, sep);
		strncat(ret, "CLOSE_NOWRITE", 14);
	}
	if (IN_OPEN & events) {
		charcat(ret, sep);
		strncat(ret, "OPEN", 5);
	}
	if (IN_MOVED_FROM & events) {
		charcat(ret, sep);
		strncat(ret, "MOVED_FROM", 11);
	}
	if (IN_MOVED_TO & events) {
		charcat(ret, sep);
		strncat(ret, "MOVED_TO", 9);
	}
	if (IN_CREATE & events) {
		charcat(ret, sep);
		strncat(ret, "CREATE", 7);
	}
	if (IN_DELETE & events) {
		charcat(ret, sep);
		strncat(ret, "DELETE", 7);
	}
	if (IN_DELETE_SELF & events) {
		charcat(ret, sep);
		strncat(ret, "DELETE_SELF", 12);
	}
	if (IN_UNMOUNT & events) {
		charcat(ret, sep);
		strncat(ret, "UNMOUNT", 8);
	}
	if (IN_Q_OVERFLOW & events) {
		charcat(ret, sep);
		strncat(ret, "Q_OVERFLOW", 11);
	}
	if (IN_IGNORED & events) {
		charcat(ret, sep);
		strncat(ret, "IGNORED", 8);
	}
	if (IN_CLOSE & events) {
		charcat(ret, sep);
		strncat(ret, "CLOSE", 6);
	}
	if (IN_MOVE_SELF & events) {
		charcat(ret, sep);
		strncat(ret, "MOVE_SELF", 10);
	}
	if (IN_ISDIR & events) {
		charcat(ret, sep);
		strncat(ret, "ISDIR", 6);
	}
	if (IN_ONESHOT & events) {
		charcat(ret, sep);
		strncat(ret, "ONESHOT", 8);
	}

	// Maybe we didn't match any... ?
	if (ret[0] == '\0') {
		niceassert(-1 != sprintf(ret, "%c0x%08x", sep, events), 0);
	}

	return &ret[1];
}

/**
 * Get the filename from fid.
 *
 * Resolve filename from fid + name and return
 * static filename string.
 */
static const char* inotifytools_filename_from_fid(
    struct fanotify_event_fid* fid) {
#ifdef LINUX_FANOTIFY
	static char filename[PATH_MAX];
	struct fanotify_event_fid fsid = {};
	int dirf = 0, mount_fd = AT_FDCWD;
	int len = 0, name_len = 0;

	// Match mount_fd from fid->fsid (and null fhandle)
	fsid.info.fsid.val[0] = fid->info.fsid.val[0];
	fsid.info.fsid.val[1] = fid->info.fsid.val[1];
	fsid.info.hdr.info_type = FAN_EVENT_INFO_TYPE_FID;
	fsid.info.hdr.len = sizeof(fsid);
	watch* mnt = watch_from_fid(&fsid);
	if (mnt)
		mount_fd = mnt->dirf;

	if (fid->info.hdr.info_type == FAN_EVENT_INFO_TYPE_DFID_NAME) {
		int fid_len = sizeof(*fid) + fid->handle.handle_bytes;

		name_len = fid->info.hdr.len - fid_len;
		if (name_len && !fid->handle.f_handle[fid->handle.handle_bytes])
			name_len = 0;  // empty name??
	}

	// Try to get path from file handle
	dirf = open_by_handle_at(mount_fd, &fid->handle, O_DIRECTORY);
	if (dirf > 0) {
		// Got path by handle
	} else if (fanotify_mark_type == FAN_MARK_FILESYSTEM) {
		// Suppress warnings for failure to decode fid for events
		// inside deleted directories
		if (errno == ESTALE)
			return "";
		fprintf(stderr, "Failed to decode directory fid (%s).\n",
			strerror(errno));
		return NULL;
	} else if (name_len) {
		// For recursive watch look for watch by fid without the name
		fid->info.hdr.info_type = FAN_EVENT_INFO_TYPE_DFID;
		fid->info.hdr.len -= name_len;

		watch* w = watch_from_fid(fid);

		fid->info.hdr.info_type = FAN_EVENT_INFO_TYPE_DFID_NAME;
		fid->info.hdr.len += name_len;

		if (!w) {
			fprintf(stderr,
				"Failed to lookup path by directory fid.\n");
			return NULL;
		}

		dirf = w->dirf ? dup(w->dirf) : -1;
		if (dirf < 0) {
			fprintf(stderr, "Failed to get directory fd.\n");
			return NULL;
		}
	} else {
		// Fallthrough to stored filename
		return NULL;
	}
	char sym[30];
	sprintf(sym, "/proc/self/fd/%d", dirf);

	// PATH_MAX - 2 because we have to append two characters to this path,
	// '/' and 0
	len = readlink(sym, filename, PATH_MAX - 2);
	if (len < 0) {
		fprintf(stderr, "Failed to resolve path from directory fd (%s).\n",
			strerror(errno));
		close(dirf);
		return NULL;
	}

	// Skip events whose path cannot be resolved via /proc/self/fd symlink,
	// such as events in paths not accessible from a bind mount which the
	// filesystem watch was set.
	if (len == 1 && filename[0] == '/') {
		filename[0] = 0;
		goto out;
	}

	filename[len++] = '/';
	filename[len] = 0;

	if (name_len > 0) {
		const char* name = (const char*)fid->handle.f_handle +
				   fid->handle.handle_bytes;
		int deleted = faccessat(dirf, name, F_OK, AT_SYMLINK_NOFOLLOW);
		if (deleted && errno != ENOENT) {
			fprintf(stderr, "Failed to access file %s (%s).\n",
				name, strerror(errno));
			close(dirf);
			return NULL;
		}
		memcpy(filename + len, name, name_len);
		if (deleted)
			strncat(filename, " (deleted)", 11);
	}
out:
	close(dirf);
	return filename;
#else
	return NULL;
#endif
}

/**
 * Get the filename from a watch.
 *
 * If not stored in watch, resolve filename from fid + name and return
 * static filename string.
 */
const char* inotifytools_filename_from_watch(watch* w) {
	if (!w)
		return "";
	if (!w->fid || !fanotify_mark_type)
		return w->filename;

	return inotifytools_filename_from_fid(w->fid) ?: w->filename;
}

/**
 * Get the filename used to establish a watch.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param wd watch descriptor.
 *
 * @return filename associated with watch descriptor @a wd, or NULL if @a wd
 *         is not associated with any filename.
 *
 * @note This always returns the filename which was used to establish a watch.
 *       This means the filename may be a relative path.  If this isn't desired,
 *       then always use absolute paths when watching files.
 *       Also, this is not necessarily the filename which might have been used
 *       to cause an event on the file, since inotify is inode based and there
 *       can be many filenames mapping to a single inode.
 *       Finally, if a file is moved or renamed while being watched, the
 *       filename returned will still be the original name.
 */
const char* inotifytools_filename_from_wd(int wd) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	if (!wd)
		return "";
	watch* w = watch_from_wd(wd);
	if (!w)
		return "";

	return inotifytools_filename_from_watch(w);
}

/**
 * Get the directory path used to establish a watch.
 *
 * Returns the filename recorded for event->wd and the dirname
 * prefix length.
 *
 * The caller should NOT free() the returned string.
 */
const char* inotifytools_dirname_from_event(struct inotify_event* event,
					    size_t* dirnamelen) {
	const char* filename = inotifytools_filename_from_wd(event->wd);
	const char* dirsep = NULL;

	if (!filename) {
		return NULL;
	}

	/* Split dirname from filename for fanotify event */
	if (fanotify_mode)
		dirsep = strrchr(filename, '/');
	if (!dirsep) {
		*dirnamelen = strlen(filename);
		return filename;
	}

	*dirnamelen = dirsep - filename + 1;
	return filename;
}

/**
 * Get the watched path and filename from an event.
 *
 * Returns the filename either recorded for event->wd or
 * from event->name and the watched filename for event->wd.
 *
 * The caller should NOT free() the returned strings.
 */
const char* inotifytools_filename_from_event(struct inotify_event* event,
					     char const** eventname,
					     size_t* dirnamelen) {
	if (event->len > 0)
		*eventname = event->name;
	else
		*eventname = "";

	const char* filename =
	    inotifytools_dirname_from_event(event, dirnamelen);

	/* On fanotify watch, filename includes event->name */
	if (filename && filename[*dirnamelen])
		*eventname = filename + *dirnamelen;

	return filename;
}

/**
 * Get the directory path from an event.
 *
 * Returns the filename recorded for event->wd or NULL.
 * For an event on non-directory also returns NULL.
 *
 * The caller is responsible to free() the returned string.
 */
char* inotifytools_dirpath_from_event(struct inotify_event* event) {
	const char* filename = inotifytools_filename_from_wd(event->wd);

	if (!filename || !*filename || !(event->mask & IN_ISDIR)) {
		return NULL;
	}

	/*
	 * fanotify watch->filename includes the name, so no need to add the
	 * event->name again.
	 */
	char* path;
	nasprintf(&path, "%s%s/", filename, fanotify_mode ? "" : event->name);

	return path;
}

/**
 * Get the watch descriptor for a particular filename.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param filename file name to find watch descriptor for.
 *
 * @return watch descriptor associated with filename, or -1 if @a filename is
 *         not associated with any watch descriptor.
 *
 * @note The filename specified must always be the original name used to
 *       establish the watch.
 */
int inotifytools_wd_from_filename(char const* filename) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	if (!filename || !*filename)
		return -1;
	watch* w = watch_from_filename(filename);
	if (!w)
		return -1;
	return w->wd;
}

/**
 * Set the filename for a particular watch descriptor.
 *
 * This function should be used to update a filename when a file is known to
 * have been moved or renamed.  At the moment, libinotifytools does not
 * automatically handle this situation.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param wd Watch descriptor.
 *
 * @param filename New filename.
 */
void inotifytools_set_filename_by_wd(int wd, char const* filename) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	watch* w = watch_from_wd(wd);
	if (!w)
		return;
	if (w->filename)
		free(w->filename);
	w->filename = strdup(filename);
}

/**
 * Set the filename for one or more watches with a particular existing filename.
 *
 * This function should be used to update a filename when a file is known to
 * have been moved or renamed.  At the moment, libinotifytools does not
 * automatically handle this situation.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param oldname Current filename.
 *
 * @param newname New filename.
 */
void inotifytools_set_filename_by_filename(char const* oldname,
					   char const* newname) {
	watch* w = watch_from_filename(oldname);
	if (!w)
		return;
	if (w->filename)
		free(w->filename);
	w->filename = strdup(newname);
}

/**
 * Replace a certain filename prefix on all watches.
 *
 * This function should be used to update filenames for an entire directory tree
 * when a directory is known to have been moved or renamed.  At the moment,
 * libinotifytools does not automatically handle this situation.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param oldname Current filename prefix.
 *
 * @param newname New filename prefix.
 *
 * @section example Example
 * @code
 * // if /home/user1/original_dir is moved to /home/user2/new_dir, then to
 * // update all watches:
 * inotifytools_replace_filename( "/home/user1/original_dir",
 *                                "/home/user2/new_dir" );
 * @endcode
 */
void inotifytools_replace_filename(char const* oldname, char const* newname) {
	if (!oldname || !newname)
		return;
	if (!*oldname || !*newname)
		return;
	struct replace_filename_data data;
	data.old_name = oldname;
	data.new_name = newname;
	data.old_len = strlen(oldname);
	rbwalk(tree_filename, replace_filename, (void*)&data);
}

/**
 * @internal
 */
int remove_inotify_watch(watch* w) {
	error = 0;
	// There is no kernel object representing the watch with fanotify
	if (w->fid)
		return 0;
	int status = inotify_rm_watch(inotify_fd, w->wd);
	if (status < 0) {
		fprintf(stderr, "Failed to remove watch on %s: %s\n",
			w->filename, strerror(status));
		error = status;
		return 0;
	}
	return 1;
}

/**
 * @internal
 */
watch* create_watch(int wd,
		    struct fanotify_event_fid* fid,
		    const char* filename,
		    int dirf) {
	if (wd < 0 || !filename || !filename[0])
		return NULL;

	watch* w = (watch*)calloc(1, sizeof(watch));
	if (!w) {
		fprintf(stderr, "Failed to allocate watch.\n");
		return NULL;
	}
	w->wd = wd ?: (unsigned long)fid;
	w->fid = fid;
	w->dirf = dirf;
	w->filename = strdup(filename);
	rbsearch(w, tree_wd);
	if (fid)
		rbsearch(w, tree_fid);

	rbsearch(w, tree_filename);
	return w;
}

/**
 * Remove a watch on a file specified by watch descriptor.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param wd Watch descriptor of watch to be removed.
 *
 * @return 1 on success, 0 on failure.  If the given watch doesn't exist,
 *         returns 1.  On failure, the error can be
 *         obtained from inotifytools_error().
 */
int inotifytools_remove_watch_by_wd(int wd) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	watch* w = watch_from_wd(wd);
	if (!w)
		return 1;

	if (!remove_inotify_watch(w))
		return 0;
	rbdelete(w, tree_wd);
	if (w->fid)
		rbdelete(w, tree_fid);
	rbdelete(w, tree_filename);
	destroy_watch(w);
	return 1;
}

/**
 * Remove a watch on a file specified by filename.
 *
 * @param filename Name of file on which watch should be removed.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().
 *
 * @note The filename specified must always be the original name used to
 *       establish the watch.
 */
int inotifytools_remove_watch_by_filename(char const* filename) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	watch* w = watch_from_filename(filename);
	if (!w)
		return 1;

	if (!remove_inotify_watch(w))
		return 0;
	rbdelete(w, tree_wd);
	if (w->fid)
		rbdelete(w, tree_fid);
	rbdelete(w, tree_filename);
	destroy_watch(w);
	return 1;
}

/**
 * Set up a watch on a file.
 *
 * @param filename Absolute or relative path of file to watch.
 *
 * @param events bitwise ORed inotify events to watch for.  See section
 *               \ref events.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().
 */
int inotifytools_watch_file(char const* filename, int events) {
	static char const* filenames[2];
	filenames[0] = filename;
	filenames[1] = NULL;
	return inotifytools_watch_files(filenames, events);
}

/**
 * Set up a watch on a list of files.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param filenames null-terminated array of absolute or relative paths of
 *                  files to watch.
 *
 * @param events bitwise OR'ed inotify events to watch for.  See section
 *               \ref events.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().
 */
int inotifytools_watch_files(char const* filenames[], int events) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	error = 0;

	static int i;
	for (i = 0; filenames[i]; ++i) {
		int wd = -1;
		if (fanotify_mode) {
#ifdef LINUX_FANOTIFY
			unsigned int flags = FAN_MARK_ADD | fanotify_mark_type;

			if (events & IN_DONT_FOLLOW) {
				events &= ~IN_DONT_FOLLOW;
				flags |= FAN_MARK_DONT_FOLLOW;
			}

			wd = fanotify_mark(inotify_fd, flags,
					   events | FAN_EVENT_ON_CHILD,
					   AT_FDCWD, filenames[i]);
#endif
		} else {
			wd =
			    inotify_add_watch(inotify_fd, filenames[i], events);
		}
		if (wd < 0) {
			if (wd == -1) {
				error = errno;
				return 0;
			}  // if ( wd == -1 )
			else {
				fprintf(
				    stderr,
				    "Failed to watch %s: returned wd was %d "
				    "(expected -1 or >0 )",
				    filenames[i], wd);
				// no appropriate value for error
				return 0;
			}  // else
		}	   // if ( wd < 0 )

		const char* filename = filenames[i];
		size_t filenamelen = strlen(filename);
		char* dirname;
		int dirf = 0;
		// Always end filename with / if it is a directory
		if (!isdir(filename)) {
			dirname = NULL;
		} else if (filename[filenamelen - 1] == '/') {
			dirname = strdup(filename);
		} else {
			nasprintf(&dirname, "%s/", filename);
			filename = dirname;
			filenamelen++;
		}

		struct fanotify_event_fid* fid = NULL;
#ifdef LINUX_FANOTIFY
		if (!wd) {
			fid = (fanotify_event_fid*)calloc(
			    1, sizeof(*fid) + MAX_FID_LEN);
			if (!fid) {
				fprintf(stderr, "Failed to allocate fid");
				free(dirname);
				return 0;
			}

			struct statfs buf;
			if (statfs(filenames[i], &buf)) {
				free(fid);
				fprintf(stderr, "Statfs failed on %s: %s\n",
					filenames[i], strerror(errno));
				free(dirname);
				return 0;
			}
			memcpy(&fid->info.fsid, &buf.f_fsid,
			       sizeof(__kernel_fsid_t));

			// For btrfs sb watch, hash only by fsid.val[0],
			// because fsid.val[1] is different per sub-volume
			if (buf.f_type == BTRFS_SUPER_MAGIC)
				fid->info.fsid.val[1] = 0;

			// Hash mount_fd with fid->fsid (and null fhandle)
			int ret, mntid;
			watch* mnt = dirname ? watch_from_fid(fid) : NULL;
			if (dirname && !mnt) {
				struct fanotify_event_fid* fsid;

				fsid = (fanotify_event_fid*)calloc(
				    1, sizeof(*fsid));
				if (!fsid) {
					free(fid);
					fprintf(stderr,
						"Failed to allocate fsid");
					free(dirname);
					return 0;
				}
				fsid->info.fsid.val[0] = fid->info.fsid.val[0];
				fsid->info.fsid.val[1] = fid->info.fsid.val[1];
				fsid->info.hdr.info_type =
				    FAN_EVENT_INFO_TYPE_FID;
				fsid->info.hdr.len = sizeof(*fsid);
				mntid = open(dirname, O_RDONLY);
				if (mntid < 0) {
					free(fid);
					free(fsid);
					fprintf(stderr,
						"Failed to open %s: %s\n",
						dirname, strerror(errno));
					free(dirname);
					return 0;
				}
				// Hash mount_fd without terminating /
				dirname[filenamelen - 1] = 0;
				create_watch(0, fsid, dirname, mntid);
				dirname[filenamelen - 1] = '/';
			}

			fid->handle.handle_bytes = MAX_FID_LEN;
name_to_handle:
			ret = name_to_handle_at(AT_FDCWD, filenames[i],
						&fid->handle, &mntid,
						at_handle_fid);
			/*
			 * Since kernel v6.6, overlayfs supports encoding file
			 * handles if using the AT_HANDLE_FID flag, so for non
			 * --filesystem watch we first try with AT_HANDLE_FID.
			 * Kernel < v6.5 does not support AT_HANDLE_FID, so fall
			 * back to encoding regular file handles in that case.
			*/
			if (ret && at_handle_fid && errno == EINVAL) {
				at_handle_fid = 0;
				goto name_to_handle;
			}
			if (ret || fid->handle.handle_bytes > MAX_FID_LEN) {
				free(fid);
				fprintf(stderr, "Encode fid failed on %s: %s\n",
					filenames[i], strerror(errno));
				free(dirname);
				return 0;
			}
			fid->info.hdr.info_type = dirname
						      ? FAN_EVENT_INFO_TYPE_DFID
						      : FAN_EVENT_INFO_TYPE_FID;
			fid->info.hdr.len =
			    sizeof(*fid) + fid->handle.handle_bytes;
			if (dirname) {
				dirf = open(dirname, O_PATH);
				if (dirf < 0) {
					free(fid);
					fprintf(stderr,
						"Failed to open %s: %s\n",
						dirname, strerror(errno));
					free(dirname);
					return 0;
				}
			}
		}
#endif
		create_watch(wd, fid, filename, dirf);
		free(dirname);
	}  // for

	return 1;
}

/**
 * Get the next inotify event to occur.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param timeout maximum amount of time, in seconds, to wait for an event.
 *                If @a timeout is non-negative, the function is non-blocking.
 *                If @a timeout is negative, the function will block until an
 *                event occurs.
 *
 * @return pointer to an inotify event, or NULL if function timed out before
 *         an event occurred.  The event is located in static storage and it
 *         may be overwritten in subsequent calls; do not call free() on it,
 *         and make a copy if you want to keep it.
 *
 * @note Your program should call this function or
 *       inotifytools_next_events() frequently; between calls to this function,
 *       inotify events will be queued in the kernel, and eventually the queue
 *       will overflow and you will miss some events.
 *
 * @note If the function inotifytools_ignore_events_by_regex() has been called
 *       with a non-NULL parameter, this function will not return on events
 *       which match the regular expression passed to that function.  However,
 *       the @a timeout period begins again each time a matching event occurs.
 */
struct inotify_event* inotifytools_next_event(long int timeout) {
	if (!timeout) {
		timeout = -1;
	}

	return inotifytools_next_events(timeout, 1);
}

/**
 * Get the next inotify events to occur.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param timeout maximum amount of time, in seconds, to wait for an event.
 *                If @a timeout is non-negative, the function is non-blocking.
 *                If @a timeout is negative, the function will block until an
 *                event occurs.
 *
 * @param num_events approximate number of inotify events to wait for until
 *                   this function returns.  Use this for buffering reads to
 *                   inotify if you expect to receive large amounts of events.
 *                   You are NOT guaranteed that this number of events will
 *                   actually be read; instead, you are guaranteed that the
 *                   number of bytes read from inotify is
 *                   @a num_events * sizeof(struct inotify_event).  Obviously
 *                   the larger this number is, the greater the latency between
 *                   when an event occurs and when you'll know about it.
 *                   May not be larger than 4096.
 *
 * @return pointer to an inotify event, or NULL if function timed out before
 *         an event occurred or @a num_events < 1.  The event is located in
 *         static storage and it may be overwritten in subsequent calls; do not
 *         call free() on it, and make a copy if you want to keep it.
 *         When @a num_events is greater than 1, this will return a pointer to
 *         the first event only, and you MUST call this function again to
 *         get pointers to subsequent events; don't try to add to the pointer
 *         to find the next events or you will run into trouble.
 *
 * @note You may actually get different events with different values of
 *       @a num_events.  This is because inotify does some in-kernel filtering
 *       of duplicate events, meaning some duplicate events will not be
 *       reported if @a num_events > 1.  For some purposes this is fine, but
 *       for others (such as gathering accurate statistics on numbers of event
 *       occurrences) you must call this function with @a num_events = 1, or
 *       simply use inotifytools_next_event().
 *
 * @note Your program should call this function frequently; between calls to
 * this function, inotify events will be queued in the kernel, and eventually
 *       the queue will overflow and you will miss some events.
 *
 * @note If the function inotifytools_ignore_events_by_regex() has been called
 *       with a non-NULL parameter, this function will not return on events
 *       which match the regular expression passed to that function.  However,
 *       the @a timeout period begins again each time a matching event occurs.
 */
struct inotify_event* inotifytools_next_events(long int timeout,
					       int num_events) {
	niceassert(initialized, "inotifytools_initialize not called yet");
	niceassert(num_events <= MAX_EVENTS, "too many events requested");

	if (num_events < 1)
		return NULL;

	// second half of event[] buffer is for fanotify->inotify conversion
	static struct inotify_event event[2 * MAX_EVENTS];
	static struct inotify_event* ret;
	static int first_byte = 0;
	static ssize_t bytes;
	static ssize_t this_bytes;
	static jmp_buf jmp;
	static struct nstring match_name;
	static char match_name_string[MAX_STRLEN + 1];

	setjmp(jmp);

	pid_t event_pid = 0;
	error = 0;

	// first_byte is index into event buffer
	if (first_byte != 0 &&
	    first_byte <= (int)(bytes - sizeof(struct inotify_event))) {
		ret = (struct inotify_event*)((char*)&event[0] + first_byte);
		if (!fanotify_mode &&
		    first_byte + sizeof(*ret) + ret->len > bytes) {
			// oh... no.  this can't be happening.  An incomplete
			// event. Copy what we currently have into first
			// element, call self to read remainder. oh, and they
			// BETTER NOT overlap. Boy I hope this code works. But I
			// think this can never happen due to how inotify is
			// written.
			niceassert((long)((char*)&event[0] +
					  sizeof(struct inotify_event) +
					  event[0].len) <= (long)ret,
				   "extremely unlucky user, death imminent");
			// how much of the event do we have?
			bytes = (char*)&event[0] + bytes - (char*)ret;
			memcpy(&event[0], ret, bytes);
			return inotifytools_next_events(timeout, num_events);
		}
		this_bytes = 0;
		goto more_events;

	}

	else if (first_byte == 0) {
		bytes = 0;
	}

	static unsigned int bytes_to_read;
	static int rc;
	static fd_set read_fds;

	static struct timeval read_timeout;
	read_timeout.tv_sec = timeout;
	read_timeout.tv_usec = 0;
	static struct timeval* read_timeout_ptr;
	read_timeout_ptr = (timeout < 0 ? NULL : &read_timeout);

	FD_ZERO(&read_fds);
	FD_SET(inotify_fd, &read_fds);
	rc = select(inotify_fd + 1, &read_fds, NULL, NULL, read_timeout_ptr);
	if (rc < 0) {
		// error
		error = errno;
		return NULL;
	} else if (rc == 0) {
		// timeout
		return NULL;
	}

	// wait until we have enough bytes to read
	do {
		rc = ioctl(inotify_fd, FIONREAD, &bytes_to_read);
	} while (!rc &&
		 bytes_to_read < sizeof(struct inotify_event) * num_events);

	if (rc == -1) {
		error = errno;
		return NULL;
	}

	this_bytes = read(inotify_fd, (char*)&event[0] + bytes,
			  sizeof(struct inotify_event) * MAX_EVENTS - bytes);
	if (this_bytes < 0) {
		error = errno;
		return NULL;
	}
	if (this_bytes == 0) {
		fprintf(stderr,
			"Inotify reported end-of-file.  Possibly too many "
			"events occurred at once.\n");
		return NULL;
	}
more_events:
	ret = (struct inotify_event*)((char*)&event[0] + first_byte);
#ifdef LINUX_FANOTIFY
	// convert fanotify events to inotify events
	if (fanotify_mode) {
		struct fanotify_event_metadata* meta =
		    (fanotify_event_metadata*)ret;
		struct fanotify_event_info_fid* info =
		    (fanotify_event_info_fid*)(meta + 1);
		struct fanotify_event_fid* fid = NULL;
		const char* name = "";
		int fid_len = 0;
		int name_len = 0;

		first_byte += meta->event_len;

		if (meta->event_len > sizeof(*meta)) {
			switch (info->hdr.info_type) {
				case FAN_EVENT_INFO_TYPE_FID:
				case FAN_EVENT_INFO_TYPE_DFID:
				case FAN_EVENT_INFO_TYPE_DFID_NAME:
					fid = (fanotify_event_fid*)info;
					fid_len = sizeof(*fid) +
						  fid->handle.handle_bytes;
					if (info->hdr.info_type ==
					    FAN_EVENT_INFO_TYPE_DFID_NAME) {
						name_len =
						    info->hdr.len - fid_len;
					}
					if (name_len > 0) {
						name =
						    (const char*)
							fid->handle.f_handle +
						    fid->handle.handle_bytes;
					}
					// Convert zero padding to zero
					// name_len. For some events on
					// directories, the fid is that of the
					// dir and name is ".". Do not include
					// "." name in fid hash, but keep it for
					// debug print.
					if (name_len &&
					    (!*name ||
					     (name[0] == '.' && !name[1]))) {
						info->hdr.len -= name_len;
						name_len = 0;
					}
					break;
			}
		}
		if (!fid) {
			fprintf(stderr, "No fid in fanotify event.\n");
			return NULL;
		}
		if (verbosity > 1) {
			printf(
			    "fanotify_event: bytes=%zd, first_byte=%d, "
			    "this_bytes=%zd, event_len=%u, fid_len=%d, "
			    "name_len=%d, name=%s\n",
			    bytes, first_byte, this_bytes, meta->event_len,
			    fid_len, name_len, name);
		}

		// For btrfs sb watch, hash only by fsid.val[0],
		// because fsid.val[1] is different on sub-volumes
		if (fid->handle.handle_type == FILEID_BTRFS_WITHOUT_PARENT)
			fid->info.fsid.val[1] = 0;

		ret = &event[MAX_EVENTS];
		watch* w = watch_from_fid(fid);
		if (!w) {
			struct fanotify_event_fid* newfid =
			    (fanotify_event_fid*)calloc(1, info->hdr.len);
			if (!newfid) {
				fprintf(stderr, "Failed to allocate fid.\n");
				return NULL;
			}
			memcpy(newfid, fid, info->hdr.len);
			const char* filename =
			    inotifytools_filename_from_fid(fid);
			if (filename && filename[0]) {
				w = create_watch(0, newfid, filename, 0);
				if (!w) {
					free(newfid);
					return NULL;
				}
			}
			// Verbose print for valid filenames and errors,
			// but not for skipped paths (empty filename).
			if ((!filename || filename[0]) && verbosity) {
				unsigned long id;
				memcpy((void*)&id, fid->handle.f_handle,
				       sizeof(id));
				printf("[fid=%x.%x.%lx;name='%s'] %s\n",
				       fid->info.fsid.val[0],
				       fid->info.fsid.val[1], id, name,
				       filename ?: "");
			}
		}
		ret->wd = w ? w->wd : 0;
		ret->mask = (uint32_t)meta->mask;
		ret->len = name_len;
		if (name_len > 0)
			memcpy(ret->name, name, name_len);
		event_pid = meta->pid;
	} else {
		first_byte += sizeof(struct inotify_event) + ret->len;
	}
#endif

	bytes += this_bytes;
	niceassert(first_byte <= bytes,
		   "ridiculously long filename, things will "
		   "almost certainly screw up.");
	if (first_byte == bytes) {
		first_byte = 0;
	}

	// Skip events from self due to open_by_handle_at()
	if (self_pid && self_pid == event_pid) {
		longjmp(jmp, 0);
	}

	// Skip events on unknown paths (e.g. when watching a bind mount)
	if (event_pid && ret->wd == 0) {
		longjmp(jmp, 0);
	}

	if (regex) {
		// Skip regex filtering for directories in recursive mode
		if (recursive_watch && (ret->mask & IN_ISDIR) &&
		    (ret->mask & (IN_CREATE | IN_MOVED_TO))) {
			// Allow directory events through when watching recursively
		} else {
			inotifytools_snprintf(&match_name, MAX_STRLEN, ret, "%w%f");
			memcpy(&match_name_string, &match_name.buf, match_name.len);
			match_name_string[match_name.len] = '\0';
			if (0 == regexec(regex, match_name_string, 0, 0, 0)) {
				if (!invert_regexp)
					longjmp(jmp, 0);
			} else {
				if (invert_regexp)
					longjmp(jmp, 0);
			}
		}
	}

	if (collect_stats) {
		record_stats(ret);
	}

	return ret;
}

/**
 * Set up recursive watches on an entire directory tree.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @param path path of directory or file to watch.  If the path is a directory,
 *             every subdirectory will also be watched for the same events up
 *             to the maximum readable depth.  If the path is a file, the file
 *             is watched exactly as if inotifytools_watch_file() were used.
 *
 * @param events Inotify events to watch for.  See section \ref events.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().  Note that some errors on
 *         subdirectories will be ignored; for example, if you watch a directory
 *         tree which contains some directories which you do not have access to,
 *         those directories will not be watched, but this function will still
 *         return 1 if no other errors occur.
 *
 * @note This function does not attempt to work atomically.  If you use this
 *       function to watch a directory tree and files or directories are being
 *       created or removed within that directory tree, there are no guarantees
 *       as to whether or not those files will be watched.
 */
int inotifytools_watch_recursively(char const* path, int events) {
	return inotifytools_watch_recursively_with_exclude(path, events, 0);
}

/**
 * Set up recursive watches on an entire directory tree, optionally excluding
 * some directories.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * @author UH
 *
 * @param path path of directory or file to watch.  If the path is a directory,
 *             every subdirectory will also be watched for the same events up
 *             to the maximum readable depth.  If the path is a file, the file
 *             is watched exactly as if inotifytools_watch_file() were used.
 *
 * @param exclude_list NULL terminated path list of directories not to watch.
 *                     Can be NULL if no paths are to be excluded.
 *                     Directories may or may not include a trailing '/'.
 *
 * @param events Inotify events to watch for.  See section \ref events.
 *
 * @return 1 on success, 0 on failure.  On failure, the error can be
 *         obtained from inotifytools_error().  Note that some errors on
 *         subdirectories will be ignored; for example, if you watch a directory
 *         tree which contains some directories which you do not have access to,
 *         those directories will not be watched, but this function will still
 *         return 1 if no other errors occur.
 *
 * @note This function does not attempt to work atomically.  If you use this
 *       function to watch a directory tree and files or directories are being
 *       created or removed within that directory tree, there are no guarantees
 *       as to whether or not those files will be watched.
 */
int inotifytools_watch_recursively_with_exclude(char const* path,
						int events,
						char const** exclude_list) {
	niceassert(initialized, "inotifytools_initialize not called yet");

	DIR* dir;
	char* my_path;
	error = 0;
	dir = opendir(path);
	if (!dir) {
		// If not a directory, don't need to do anything special
		if (errno == ENOTDIR) {
			return inotifytools_watch_file(path, events);
		} else {
			error = errno;
			return 0;
		}
	}

	if (path[strlen(path) - 1] != '/') {
		nasprintf(&my_path, "%s/", path);
	} else {
		my_path = (char*)path;
	}

	static struct dirent* ent;
	char* next_file;
	static struct stat my_stat;
	ent = readdir(dir);
	// Watch each directory within this directory
	while (ent) {
		if ((0 != strcmp(ent->d_name, ".")) &&
		    (0 != strcmp(ent->d_name, ".."))) {
			nasprintf(&next_file, "%s%s", my_path, ent->d_name);
			if (-1 == lstat(next_file, &my_stat)) {
				error = errno;
				free(next_file);
				if (errno != EACCES) {
					error = errno;
					if (my_path != path)
						free(my_path);
					closedir(dir);
					return 0;
				}
			} else if (S_ISDIR(my_stat.st_mode) &&
				   !S_ISLNK(my_stat.st_mode)) {
				free(next_file);
				nasprintf(&next_file, "%s%s/", my_path,
					  ent->d_name);
				static unsigned int no_watch;
				static char const** exclude_entry;

				no_watch = 0;
				for (exclude_entry = exclude_list;
				     exclude_entry && *exclude_entry &&
				     !no_watch;
				     ++exclude_entry) {
					static int exclude_length;

					exclude_length = strlen(*exclude_entry);
					if ((*exclude_entry)[exclude_length -
							     1] == '/') {
						--exclude_length;
					}
					if (strlen(next_file) ==
						(unsigned)(exclude_length +
							   1) &&
					    !strncmp(*exclude_entry, next_file,
						     exclude_length)) {
						// directory found in exclude
						// list
						no_watch = 1;
					}
				}
				if (!no_watch) {
					static int status;
					status =
					    inotifytools_watch_recursively_with_exclude(
						next_file, events,
						exclude_list);
					// For some errors, we will continue.
					if (!status && (EACCES != error) &&
					    (ENOENT != error) &&
					    (ELOOP != error)) {
						free(next_file);
						if (my_path != path)
							free(my_path);
						closedir(dir);
						return 0;
					}
				}  // if !no_watch
				free(next_file);
			}  // if isdir and not islnk
			else {
				free(next_file);
			}
		}
		ent = readdir(dir);
		error = 0;
	}

	closedir(dir);

	int ret = inotifytools_watch_file(my_path, events);
	if (my_path != path)
		free(my_path);
	return ret;
}

/**
 * Get the last error which occurred.
 *
 * When a function fails, call this to find out why.  The returned value is
 * a typical @a errno value, the meaning of which depends on context.  For
 * example, if inotifytools_watch_file() fails because you attempt to watch
 * a file which doesn't exist, this function will return @a ENOENT.
 *
 * @return an error code.
 */
int inotifytools_error() {
	return error;
}

/**
 * @internal
 */
static int isdir(char const* path) {
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

/**
 * Get the number of watches set up through libinotifytools.
 *
 * @return number of watches set up by inotifytools_watch_file(),
 *         inotifytools_watch_files() and inotifytools_watch_recursively().
 */
int inotifytools_get_num_watches() {
	int ret = 0;
	rbwalk(tree_filename, get_num, (void*)&ret);
	return ret;
}

/**
 * Print a string to standard out using an inotify_event and a printf-like
 * syntax.
 * The string written will only ever be up to 4096 characters in length.
 *
 * @param event the event to use to construct a string.
 *
 * @param fmt the format string used to construct a string.
 *
 * @return number of characters written, or -1 if an error occurs.
 *
 * @section syntax Format string syntax
 * The following tokens will be replaced with the specified string:
 *  \li \c \%w - This will be replaced with the name of the Watched file on
 *               which an event occurred.
 *  \li \c \%c - This will be replaced with the cookie of the Watched file on
 *               which an event occurred.
 *  \li \c \%f - When an event occurs within a directory, this will be replaced
 *               with the name of the File which caused the event to occur.
 *               Otherwise, this will be replaced with an empty string.
 *  \li \c \%e - Replaced with the Event(s) which occurred, comma-separated.
 *  \li \c \%Xe - Replaced with the Event(s) which occurred, separated by
 *                whichever character is in the place of `X'.
 *  \li \c \%T - Replaced by the current Time in the format specified by the
 *               string previously passed to inotifytools_set_printf_timefmt(),
 *               or replaced with an empty string if that function has never
 *               been called.
 *  \li \c \%0 - Replaced with the 'NUL' character
 *  \li \c \%n - Replaced with the 'Line Feed' character
 *
 * @section example Example
 * @code
 * // suppose this is the only file watched.
 * inotifytools_watch_file( "mydir/", IN_CLOSE );
 *
 * // wait until an event occurs
 * struct inotify_event * event = inotifytools_next_event( -1 );
 *
 * inotifytools_printf(stderr, event, "in %w, file %f had event(s): %.e\n");
 * // suppose the file 'myfile' in mydir was read from and closed.  Then,
 * // this prints to standard out something like:
 * // "in mydir/, file myfile had event(s): CLOSE_NOWRITE.CLOSE.ISDIR\n"
 * @endcode
 */
int inotifytools_printf(struct inotify_event* event, const char* fmt) {
	return inotifytools_fprintf(stdout, event, fmt);
}

/**
 * Print a string to a file using an inotify_event and a printf-like syntax.
 * The string written will only ever be up to 4096 characters in length.
 *
 * @param file file to print to
 *
 * @param event the event to use to construct a string.
 *
 * @param fmt the format string used to construct a string.
 *
 * @return number of characters written, or -1 if an error occurs.
 *
 * @section syntax Format string syntax
 * The following tokens will be replaced with the specified string:
 *  \li \c \%w - This will be replaced with the name of the Watched file on
 *               which an event occurred.
 *  \li \c \%c - This will be replaced with the cookie of the Watched file on
 *               which an event occurred.
 *  \li \c \%f - When an event occurs within a directory, this will be replaced
 *               with the name of the File which caused the event to occur.
 *               Otherwise, this will be replaced with an empty string.
 *  \li \c \%e - Replaced with the Event(s) which occurred, comma-separated.
 *  \li \c \%Xe - Replaced with the Event(s) which occurred, separated by
 *                whichever character is in the place of `X'.
 *  \li \c \%T - Replaced by the current Time in the format specified by the
 *               string previously passed to inotifytools_set_printf_timefmt(),
 *               or replaced with an empty string if that function has never
 *               been called.
 *  \li \c \%0 - Replaced with the 'NUL' character
 *  \li \c \%n - Replaced with the 'Line Feed' character
 *
 * @section example Example
 * @code
 * // suppose this is the only file watched.
 * inotifytools_watch_file( "mydir/", IN_CLOSE );
 *
 * // wait until an event occurs
 * struct inotify_event * event = inotifytools_next_event( -1 );
 *
 * inotifytools_fprintf(stderr, event, "in %w, file %f had event(s): %.e\n");
 * // suppose the file 'myfile' in mydir was read from and closed.  Then,
 * // this prints to standard error something like:
 * // "in mydir/, file myfile had event(s): CLOSE_NOWRITE.CLOSE.ISDIR\n"
 * @endcode
 */
int inotifytools_fprintf(FILE* file,
			 struct inotify_event* event,
			 const char* fmt) {
	static struct nstring out;
	static int ret;
	ret = inotifytools_sprintf(&out, event, fmt);
	if (-1 != ret)
		fwrite(out.buf, sizeof(char), out.len, file);
	return ret;
}

/**
 * Construct a string using an inotify_event and a printf-like syntax.
 * The string can only ever be up to 4096 characters in length.
 *
 * This function will keep writing until it reaches 4096 characters.  If your
 * allocated array is not large enough to hold the entire string, your program
 * may crash.
 * inotifytools_snprintf() is safer and you should use it where possible.
 *
 * @param out location in which to store nstring.
 *
 * @param event the event to use to construct a nstring.
 *
 * @param fmt the format string used to construct a nstring.
 *
 * @return number of characters written, or -1 if an error occurs.
 *
 * @section syntax Format string syntax
 * The following tokens will be replaced with the specified string:
 *  \li \c \%w - This will be replaced with the name of the Watched file on
 *               which an event occurred.
 *  \li \c \%c - This will be replaced with the cookie of the Watched file on
 *               which an event occurred.
 *  \li \c \%f - When an event occurs within a directory, this will be replaced
 *               with the name of the File which caused the event to occur.
 *               Otherwise, this will be replaced with an empty string.
 *  \li \c \%e - Replaced with the Event(s) which occurred, comma-separated.
 *  \li \c \%Xe - Replaced with the Event(s) which occurred, separated by
 *                whichever character is in the place of `X'.
 *  \li \c \%T - Replaced by the current Time in the format specified by the
 *               string previously passed to inotifytools_set_printf_timefmt(),
 *               or replaced with an empty string if that function has never
 *               been called.
 *  \li \c \%0 - Replaced with the 'NUL' character
 *  \li \c \%n - Replaced with the 'Line Feed' character
 *
 * @section example Example
 * @code
 * // suppose this is the only file watched.
 * inotifytools_watch_file( "mydir/", IN_CLOSE );
 *
 * // wait until an event occurs
 * struct inotify_event * event = inotifytools_next_event( -1 );
 *
 * nstring mynstring;
 * inotifytools_sprintf(mynstring, event, "in %w, file %f had event(s): %.e\n");
 * fwrite( mynstring.buf, sizeof(char), mynstring.len, stdout );
 * // suppose the file 'myfile' in mydir was written to and closed.  Then,
 * // this prints something like:
 * // "in mydir/, file myfile had event(s): CLOSE_WRITE.CLOSE.ISDIR\n"
 * @endcode
 */
int inotifytools_sprintf(struct nstring* out,
			 struct inotify_event* event,
			 const char* fmt) {
	return inotifytools_snprintf(out, MAX_STRLEN, event, fmt);
}

/**
 * Construct a string using an inotify_event and a printf-like syntax.
 * The string can only ever be up to 4096 characters in length.
 *
 * @param out location in which to store nstring.
 *
 * @param size maximum amount of characters to write.
 *
 * @param event the event to use to construct a nstring.
 *
 * @param fmt the format string used to construct a nstring.
 *
 * @return number of characters written, or -1 if an error occurs.
 *
 * @section syntax Format string syntax
 * The following tokens will be replaced with the specified string:
 *  \li \c \%w - This will be replaced with the name of the Watched file on
 *               which an event occurred.
 *  \li \c \%c - This will be replaced with cookie of the Watched file on
 *               which an event occurred.
 *  \li \c \%f - When an event occurs within a directory, this will be replaced
 *               with the name of the File which caused the event to occur.
 *               Otherwise, this will be replaced with an empty string.
 *  \li \c \%e - Replaced with the Event(s) which occurred, comma-separated.
 *  \li \c \%Xe - Replaced with the Event(s) which occurred, separated by
 *                whichever character is in the place of `X'.
 *  \li \c \%T - Replaced by the current Time in the format specified by the
 *               string previously passed to inotifytools_set_printf_timefmt(),
 *               or replaced with an empty string if that function has never
 *               been called.
 *  \li \c \%0 - Replaced with the 'NUL' character
 *  \li \c \%n - Replaced with the 'Line Feed' character
 *
 * @section example Example
 * @code
 * // suppose this is the only file watched.
 * inotifytools_watch_file( "mydir/", IN_CLOSE );
 *
 * // wait until an event occurs
 * struct inotify_event * event = inotifytools_next_event( -1 );
 *
 * struct nstring mynstring;
 * inotifytools_snprintf( mynstring, MAX_STRLEN, event,
 *                        "in %w, file %f had event(s): %.e\n" );
 * fwrite( mynstring.buf, sizeof(char), mynstring.len, stdout );
 * // suppose the file 'myfile' in mydir was written to and closed.  Then,
 * // this prints something like:
 * // "in mydir/, file myfile had event(s): CLOSE_WRITE.CLOSE.ISDIR\n"
 * @endcode
 */
int inotifytools_snprintf(struct nstring* out,
			  int size,
			  struct inotify_event* event,
			  const char* fmt) {
	const char* eventstr;
	static unsigned int i, ind;
	static char ch1;
	static char timestr[MAX_STRLEN];
	static time_t now;

	size_t dirnamelen = 0;
	const char* eventname;
	const char* filename =
	    inotifytools_filename_from_event(event, &eventname, &dirnamelen);

	if (!fmt || 0 == strlen(fmt)) {
		error = EINVAL;
		return -1;
	}
	if (strlen(fmt) > MAX_STRLEN || size > MAX_STRLEN) {
		error = EMSGSIZE;
		return -1;
	}

	ind = 0;
	for (i = 0; i < strlen(fmt) && (int)ind < size - 1; ++i) {
		if (fmt[i] != '%') {
			out->buf[ind++] = fmt[i];
			continue;
		}

		if (i == strlen(fmt) - 1) {
			// last character is %, invalid
			error = EINVAL;
			return ind;
		}

		ch1 = fmt[i + 1];

		if (ch1 == '%') {
			out->buf[ind++] = '%';
			++i;
			continue;
		}

		if (ch1 == '0') {
			out->buf[ind++] = '\0';
			++i;
			continue;
		}

		if (ch1 == 'n') {
			out->buf[ind++] = '\n';
			++i;
			continue;
		}

		if (ch1 == 'w') {
			if (filename && dirnamelen <= size - ind) {
				strncpy(&out->buf[ind], filename, dirnamelen);
				ind += dirnamelen;
			}
			++i;
			continue;
		}

		if (ch1 == 'f') {
			if (eventname) {
				strncpy(&out->buf[ind], eventname, size - ind);
				ind += strlen(eventname);
			}
			++i;
			continue;
		}

		if (ch1 == 'c') {
			ind += snprintf(&out->buf[ind], size - ind, "%x",
					event->cookie);
			++i;
			continue;
		}

		if (ch1 == 'e') {
			eventstr = inotifytools_event_to_str(event->mask);
			strncpy(&out->buf[ind], eventstr, size - ind);
			ind += strlen(eventstr);
			++i;
			continue;
		}

		if (ch1 == 'T') {
			if (!timefmt.empty()) {
				now = time(0);
				struct tm now_tm;
				if (!strftime(timestr, MAX_STRLEN - 1,
					      timefmt.c_str_,
					      localtime_r(&now, &now_tm))) {
					// time format probably invalid
					error = EINVAL;
					return ind;
				}
			} else {
				timestr[0] = 0;
			}

			strncpy(&out->buf[ind], timestr, size - ind);
			ind += strlen(timestr);
			++i;
			continue;
		}

		// Check if next char in fmt is e
		if (i < strlen(fmt) - 2 && fmt[i + 2] == 'e') {
			eventstr =
			    inotifytools_event_to_str_sep(event->mask, ch1);
			strncpy(&out->buf[ind], eventstr, size - ind);
			ind += strlen(eventstr);
			i += 2;
			continue;
		}

		// OK, this wasn't a special format character, just output it as
		// normal
		if (ind < MAX_STRLEN)
			out->buf[ind++] = '%';
		if (ind < MAX_STRLEN)
			out->buf[ind++] = ch1;
		++i;
	}
	out->len = ind;

	return ind - 1;
}

/**
 * Set time format for printf functions.
 *
 * @param fmt A format string valid for use with strftime, or NULL.  If NULL,
 *            time substitutions will no longer be made in printf functions.
 *            Note that this format string is not validated at all; using an
 *            incorrect format string will cause the printf functions to give
 *            incorrect results.
 */
void inotifytools_set_printf_timefmt(const char* fmt) {
	timefmt.set_size(nasprintf(&timefmt.c_str_, "%s", fmt));
}

void inotifytools_clear_timefmt() {
	timefmt.clear();
}

/**
 * Get the event queue size.
 *
 * This setting can also be read or modified by accessing the file
 * \a /proc/sys/fs/inotify/max_queued_events.
 *
 * @return the maximum number of events which will be queued in the kernel.
 */
int inotifytools_get_max_queued_events() {
	int ret;
	if (!read_num_from_file(QUEUE_SIZE_PATH, &ret))
		return -1;
	return ret;
}

/**
 * Get the maximum number of user instances of inotify.
 *
 * This setting can also be read or modified by accessing the file
 * \a /proc/sys/fs/inotify/max_user_instances.
 *
 * @return the maximum number of inotify file descriptors a single user can
 *         obtain.
 */
int inotifytools_get_max_user_instances() {
	int ret;
	if (!read_num_from_file(INSTANCES_PATH, &ret))
		return -1;
	return ret;
}

/**
 * Get the maximum number of user watches.
 *
 * This setting can also be read or modified by accessing the file
 * \a /proc/sys/fs/inotify/max_user_watches.
 *
 * @return the maximum number of inotify watches a single user can obtain per
 *         inotify instance.
 */
int inotifytools_get_max_user_watches() {
	int ret;
	if (!read_num_from_file(WATCHES_SIZE_PATH, &ret))
		return -1;
	return ret;
}

/**
 * Ignore inotify events matching a particular regular expression.
 *
 * @a pattern is a regular expression and @a flags is a bitwise combination of
 * POSIX regular expression flags. @a invert determines the type of filtering:
 * 0 (--exclude[i]): exclude all files matching @a pattern
 * 1 (--include[i]): exclude all files except those matching @a pattern
 *
 * On future calls to inotifytools_next_events() or inotifytools_next_event(),
 * the regular expression is executed on the filename of files on which
 * events occur.  If the regular expression matches, the matched event will be
 * ignored.
 */
static int do_ignore_events_by_regex(char const* pattern,
				     int flags,
				     int invert,
				     int recursive) {
	if (!pattern) {
		if (regex) {
			regfree(regex);
			free(regex);
			regex = 0;
		}
		return 1;
	}

	if (regex) {
		regfree(regex);
	} else {
		regex = (regex_t*)malloc(sizeof(regex_t));
	}

	invert_regexp = invert;
	recursive_watch = recursive;

	int ret = regcomp(regex, pattern, flags | REG_NOSUB);
	if (0 == ret)
		return 1;

	regfree(regex);
	free(regex);
	regex = 0;
	error = EINVAL;
	return 0;
}

/**
 * Ignore inotify events matching a particular regular expression.
 *
 * @a pattern is a regular expression and @a flags is a bitwise combination of
 * POSIX regular expression flags.
 *
 * On future calls to inotifytools_next_events() or inotifytools_next_event(),
 * the regular expression is executed on the filename of files on which
 * events occur.  If the regular expression matches, the matched event will be
 * ignored.
 */
int inotifytools_ignore_events_by_regex(char const* pattern, int flags, int recursive) {
	return do_ignore_events_by_regex(pattern, flags, 0, recursive);
}

/**
 * Ignore inotify events NOT matching a particular regular expression.
 *
 * @a pattern is a regular expression and @a flags is a bitwise combination of
 * POSIX regular expression flags.
 *
 * On future calls to inotifytools_next_events() or inotifytools_next_event(),
 * the regular expression is executed on the filename of files on which
 * events occur.  If the regular expression matches, the matched event will be
 * ignored.
 */
int inotifytools_ignore_events_by_inverted_regex(char const* pattern, int flags, int recursive) {
	return do_ignore_events_by_regex(pattern, flags, 1, recursive);
}

int event_compare(const char* p1, const char* p2, const void* config) {
	if (!p1 || !p2)
		return p1 - p2;
	char asc = 1;
	long sort_event = (long)config;
	if (sort_event == -1) {
		sort_event = 0;
		asc = 0;
	} else if (sort_event < 0) {
		sort_event = -sort_event;
		asc = 0;
	}
	unsigned int* i1 = stat_ptr((watch*)p1, sort_event);
	unsigned int* i2 = stat_ptr((watch*)p2, sort_event);
	if (0 == *i1 - *i2) {
		return ((watch*)p1)->wd - ((watch*)p2)->wd;
	}
	if (asc)
		return *i1 - *i2;
	else
		return *i2 - *i1;
}

struct rbtree* inotifytools_wd_sorted_by_event(int sort_event) {
	struct rbtree* ret =
	    rbinit(event_compare, (void*)(uintptr_t)sort_event);
	RBLIST* all = rbopenlist(tree_wd);
	void const* p = rbreadlist(all);
	while (p) {
		void const* r = rbsearch(p, ret);
		niceassert((int)(r == p),
			   "Couldn't insert watch into new tree");
		p = rbreadlist(all);
	}
	rbcloselist(all);
	return ret;
}
