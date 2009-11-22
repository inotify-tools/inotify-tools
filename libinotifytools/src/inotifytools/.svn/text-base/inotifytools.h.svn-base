#ifndef _inotifytools_H
#define _inotifytools_H

#ifdef __cplusplus
extern "C"
{
#endif

#include <stdio.h>

int inotifytools_str_to_event(char const * event);
int inotifytools_str_to_event_sep(char const * event, char sep);
char * inotifytools_event_to_str(int events);
char * inotifytools_event_to_str_sep(int events, char sep);
void inotifytools_set_filename_by_wd( int wd, char const * filename );
void inotifytools_set_filename_by_filename( char const * oldname,
                                            char const * newname );
void inotifytools_replace_filename( char const * oldname,
                                    char const * newname );
char * inotifytools_filename_from_wd( int wd );
int inotifytools_wd_from_filename( char const * filename );
int inotifytools_remove_watch_by_filename( char const * filename );
int inotifytools_remove_watch_by_wd( int wd );
int inotifytools_watch_file( char const * filename, int events );
int inotifytools_watch_files( char const * filenames[], int events );
int inotifytools_watch_recursively( char const * path, int events );
int inotifytools_watch_recursively_with_exclude( char const * path,
                                                 int events,
                                                 char const ** exclude_list );
                                                 // [UH]
int inotifytools_ignore_events_by_regex( char const *pattern, int flags );
struct inotify_event * inotifytools_next_event( int timeout );
struct inotify_event * inotifytools_next_events( int timeout, int num_events );
int inotifytools_error();
int inotifytools_get_stat_by_wd( int wd, int event );
int inotifytools_get_stat_total( int event );
int inotifytools_get_stat_by_filename( char const * filename,
                                                int event );
void inotifytools_initialize_stats();
int inotifytools_initialize();
void inotifytools_cleanup();
int inotifytools_get_num_watches();

int inotifytools_printf( struct inotify_event* event, char* fmt );
int inotifytools_fprintf( FILE* file, struct inotify_event* event, char* fmt );
int inotifytools_sprintf( char * out, struct inotify_event* event, char* fmt );
int inotifytools_snprintf( char * out, int size, struct inotify_event* event,
                           char* fmt );
void inotifytools_set_printf_timefmt( char * fmt );

int inotifytools_get_max_user_watches();
int inotifytools_get_max_user_instances();
int inotifytools_get_max_queued_events();

#ifdef __cplusplus
}
#endif

#endif     // _inotifytools_H
