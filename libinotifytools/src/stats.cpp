#include "stats.h"

static unsigned num_access;
static unsigned num_modify;
static unsigned num_attrib;
static unsigned num_close_nowrite;
static unsigned num_close_write;
static unsigned num_open;
static unsigned num_move_self;
static unsigned num_moved_to;
static unsigned num_moved_from;
static unsigned num_create;
static unsigned num_delete;
static unsigned num_delete_self;
static unsigned num_unmount;
static unsigned num_total;

/**
 * @internal
 */
void empty_stats(const void* nodep,
		 const VISIT which,
		 const int depth,
		 void* arg) {
	if (which != endorder && which != leaf)
		return;
	watch* w = (watch*)nodep;
	w->hit_access = 0;
	w->hit_modify = 0;
	w->hit_attrib = 0;
	w->hit_close_nowrite = 0;
	w->hit_close_write = 0;
	w->hit_open = 0;
	w->hit_move_self = 0;
	w->hit_moved_from = 0;
	w->hit_moved_to = 0;
	w->hit_create = 0;
	w->hit_delete = 0;
	w->hit_delete_self = 0;
	w->hit_unmount = 0;
	w->hit_total = 0;
}

/**
 * @internal
 */
void record_stats(struct inotify_event const* event) {
	if (!event)
		return;
	watch* w = watch_from_wd(event->wd);
	if (!w)
		return;
	if (IN_ACCESS & event->mask) {
		++w->hit_access;
		++num_access;
	}
	if (IN_MODIFY & event->mask) {
		++w->hit_modify;
		++num_modify;
	}
	if (IN_ATTRIB & event->mask) {
		++w->hit_attrib;
		++num_attrib;
	}
	if (IN_CLOSE_WRITE & event->mask) {
		++w->hit_close_write;
		++num_close_write;
	}
	if (IN_CLOSE_NOWRITE & event->mask) {
		++w->hit_close_nowrite;
		++num_close_nowrite;
	}
	if (IN_OPEN & event->mask) {
		++w->hit_open;
		++num_open;
	}
	if (IN_MOVED_FROM & event->mask) {
		++w->hit_moved_from;
		++num_moved_from;
	}
	if (IN_MOVED_TO & event->mask) {
		++w->hit_moved_to;
		++num_moved_to;
	}
	if (IN_CREATE & event->mask) {
		++w->hit_create;
		++num_create;
	}
	if (IN_DELETE & event->mask) {
		++w->hit_delete;
		++num_delete;
	}
	if (IN_DELETE_SELF & event->mask) {
		++w->hit_delete_self;
		++num_delete_self;
	}
	if (IN_UNMOUNT & event->mask) {
		++w->hit_unmount;
		++num_unmount;
	}
	if (IN_MOVE_SELF & event->mask) {
		++w->hit_move_self;
		++num_move_self;
	}

	++w->hit_total;
	++num_total;
}

unsigned int* stat_ptr(watch* w, int event) {
	if (IN_ACCESS == event)
		return &w->hit_access;
	if (IN_MODIFY == event)
		return &w->hit_modify;
	if (IN_ATTRIB == event)
		return &w->hit_attrib;
	if (IN_CLOSE_WRITE == event)
		return &w->hit_close_write;
	if (IN_CLOSE_NOWRITE == event)
		return &w->hit_close_nowrite;
	if (IN_OPEN == event)
		return &w->hit_open;
	if (IN_MOVED_FROM == event)
		return &w->hit_moved_from;
	if (IN_MOVED_TO == event)
		return &w->hit_moved_to;
	if (IN_CREATE == event)
		return &w->hit_create;
	if (IN_DELETE == event)
		return &w->hit_delete;
	if (IN_DELETE_SELF == event)
		return &w->hit_delete_self;
	if (IN_UNMOUNT == event)
		return &w->hit_unmount;
	if (IN_MOVE_SELF == event)
		return &w->hit_move_self;
	if (0 == event)
		return &w->hit_total;
	return 0;
}

/**
 * Get statistics by a particular watch descriptor.
 *
 * inotifytools_initialize_stats() must be called before this function can
 * be used.
 *
 * @param wd watch descriptor to get stats for.
 *
 * @param event a single inotify event to get statistics for, or 0 for event
 *              total.  See section \ref events.
 *
 * @return the number of times the event specified by @a event has occurred on
 *         the watch descriptor specified by @a wd since stats collection was
 *         enabled, or -1 if @a event or @a wd are invalid.
 */
int inotifytools_get_stat_by_wd(int wd, int event) {
	if (!collect_stats)
		return -1;

	watch* w = watch_from_wd(wd);
	if (!w)
		return -1;
	unsigned int* i = stat_ptr(w, event);
	if (!i)
		return -1;
	return *i;
}

/**
 * Get statistics aggregated across all watches.
 *
 * inotifytools_initialize_stats() must be called before this function can
 * be used.
 *
 * @param event a single inotify event to get statistics for, or 0 for event
 *              total.  See section \ref events.
 *
 * @return the number of times the event specified by @a event has occurred over
 *         all watches since stats collection was enabled, or -1 if @a event
 *         is not a valid event.
 */
int inotifytools_get_stat_total(int event) {
	if (!collect_stats)
		return -1;
	if (IN_ACCESS == event)
		return num_access;
	if (IN_MODIFY == event)
		return num_modify;
	if (IN_ATTRIB == event)
		return num_attrib;
	if (IN_CLOSE_WRITE == event)
		return num_close_write;
	if (IN_CLOSE_NOWRITE == event)
		return num_close_nowrite;
	if (IN_OPEN == event)
		return num_open;
	if (IN_MOVED_FROM == event)
		return num_moved_from;
	if (IN_MOVED_TO == event)
		return num_moved_to;
	if (IN_CREATE == event)
		return num_create;
	if (IN_DELETE == event)
		return num_delete;
	if (IN_DELETE_SELF == event)
		return num_delete_self;
	if (IN_UNMOUNT == event)
		return num_unmount;
	if (IN_MOVE_SELF == event)
		return num_move_self;

	if (0 == event)
		return num_total;

	return -1;
}

/**
 * Get statistics by a particular filename.
 *
 * inotifytools_initialize_stats() must be called before this function can
 * be used.
 *
 * @param filename name of file to get stats for.
 *
 * @param event a single inotify event to get statistics for, or 0 for event
 *              total.  See section \ref events.
 *
 * @return the number of times the event specified by @a event has occurred on
 *         the file specified by @a filename since stats collection was
 *         enabled, or -1 if the file is not being watched or @a event is
 *         invalid.
 *
 * @note The filename specified must always be the original name used to
 *       establish the watch.
 */
int inotifytools_get_stat_by_filename(char const* filename, int event) {
	return inotifytools_get_stat_by_wd(
	    inotifytools_wd_from_filename(filename), event);
}

/**
 * Initialize or reset statistics.
 *
 * inotifytools_initialize() must be called before this function can
 * be used.
 *
 * When this function is called, all subsequent events will be tallied.
 * Statistics can then be obtained via the @a inotifytools_get_stat_* functions.
 *
 * After the first call, subsequent calls to this function will reset the
 * event tallies to 0.
 */
void inotifytools_initialize_stats() {
	niceassert(initialized, "inotifytools_initialize not called yet");

	// if already collecting stats, reset stats
	if (collect_stats) {
		rbwalk(tree_wd, empty_stats, 0);
	}

	num_access = 0;
	num_modify = 0;
	num_attrib = 0;
	num_close_nowrite = 0;
	num_close_write = 0;
	num_open = 0;
	num_move_self = 0;
	num_moved_from = 0;
	num_moved_to = 0;
	num_create = 0;
	num_delete = 0;
	num_delete_self = 0;
	num_unmount = 0;
	num_total = 0;

	collect_stats = 1;
}
