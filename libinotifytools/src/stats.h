#ifndef STATS_H
#define STATS_H
#include "inotifytools.h"
#include "inotifytools/inotify.h"
#include "inotifytools_p.h"


extern int collect_stats;
void record_stats( struct inotify_event const * event );
unsigned int *stat_ptr(watch *w, int event);
watch *watch_from_wd(int wd);
#endif // STATS_H