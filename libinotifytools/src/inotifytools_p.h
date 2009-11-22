#ifndef INOTIFYTOOLS_P_H
#define INOTIFYTOOLS_P_H

#include "redblack.h"

struct rbtree *inotifytools_wd_sorted_by_event(int sort_event);

typedef struct watch {
	char *filename;
	int wd;
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

#endif
