#ifndef INOTIFYTOOLS_P_H
#define INOTIFYTOOLS_P_H

#include "redblack.h"

/**
 * @internal
 * Assert that a condition evaluates to true, and optionally output a message
 * if the assertion fails.
 *
 * @param  cond  Integer; if 0, assertion fails, otherwise assertion succeeds.
 *
 * @param  mesg  A human-readable error message shown if assertion fails.
 *
 * @section example Example
 * @code
 * int upper = 100, lower = 50;
 * int input = get_user_input();
 * niceassert( input <= upper && input >= lower,
 *             "input not in required range!");
 * @endcode
 */
#define niceassert(cond,mesg) _niceassert((long)cond, __LINE__, __FILE__, \
                                          #cond, mesg)


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
		 char const* mesg);

struct rbtree *inotifytools_wd_sorted_by_event(int sort_event);
extern int initialized;

struct fanotify_event_fid;

#define MAX_FID_LEN 20

typedef struct watch {
	struct fanotify_event_fid* fid;
	char *filename;
	unsigned long wd;
	int dirf;
	unsigned hit_access;
	unsigned hit_modify;
	unsigned hit_attrib;
	unsigned hit_close_write;
	unsigned hit_close_nowrite;
	unsigned hit_open;
	unsigned hit_moved_from;
	unsigned hit_moved_to;
	unsigned hit_create;
	unsigned hit_delete;
	unsigned hit_delete_self;
	unsigned hit_unmount;
	unsigned hit_move_self;
	unsigned hit_total;
} watch;
extern struct rbtree *tree_wd;
#endif
