#include "inotifytools/inotify.h"
#include "inotifytools/inotifytools.h"

#include "../../config.h"

#include <unistd.h>

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>

#ifdef HAVE_MCHECK_H
#include <mcheck.h>
#endif

#define verify(A)                                                              \
    if (!_verify((int)(A), #A, __FILE__, __LINE__, __PRETTY_FUNCTION__))       \
        return;
#define verify2(A, B)                                                          \
    if (!_verify((int)(A), B, __FILE__, __LINE__, __PRETTY_FUNCTION__))        \
        return;
#define compare(A, B)                                                          \
    if (!_compare((uintptr_t)(A), (int)(B), #A, #B, __FILE__, __LINE__,        \
                  __PRETTY_FUNCTION__))                                        \
        return;

#define succeed()                                                              \
    do {                                                                       \
        ++tests_succeeded;                                                     \
        /*fprintf(stderr, "%s:%d Test '%s' passed", file, line, func);*/       \
        /*fprintf(stderr, "\n");*/                                             \
    } while (0)

#define fail(...)                                                              \
    do {                                                                       \
        ++tests_failed;                                                        \
        fprintf(stderr, "%s:%d Test '%s' failed: ", file, line, func);         \
        fprintf(stderr, __VA_ARGS__);                                          \
        fprintf(stderr, "\n");                                                 \
        if (inotifytools_error() != 0) {                                       \
            fprintf(stderr, "inotifytools_error() returns %d (%s)\n",          \
                    inotifytools_error(), strerror(inotifytools_error()));     \
        }                                                                      \
    } while (0)

#define TEST_DIR "/tmp/inotifytools_test"

#define INFO(...)                                                              \
    do {                                                                       \
        printf("%s: ", __PRETTY_FUNCTION__);                                   \
        printf(__VA_ARGS__);                                                   \
    } while (0)

#define ENTER INFO("test begin\n");
#define EXIT INFO("test end\n");

static int tests_failed;
static int tests_succeeded;

int _verify(int cond, char const *teststr, char const *file, int line,
            char const *func) {
    if (cond) {
        succeed();
        return cond;
    }

    fail("Verification failed (%s)", teststr);
    return cond;
}

int _compare(int value, int expval, char const *act_str, char const *exp_str,
             char const *file, int line, char const *func) {
    if (value == expval) {
        succeed();
        return 1;
    }

    fail("Actual (%s): %d, Expected (%s): %d", act_str, value, exp_str, expval);
    return 0;
}

void event_to_str() {
    ENTER
    verify2(
        !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                "OPEN,MODIFY,ACCESS") ||
            !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                    "OPEN,ACCESS,MODIFY") ||
            !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                    "ACCESS,OPEN,MODIFY") ||
            !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                    "MODIFY,OPEN,ACCESS") ||
            !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                    "ACCESS,MODIFY,OPEN") ||
            !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
                    "MODIFY,ACCESS,OPEN"),
        inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS));
    EXIT
}

void event_to_str_sep() {
    ENTER
    verify2(
        !strcmp(
            inotifytools_event_to_str_sep(IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
            "OPEN:MODIFY:ACCESS") ||
            !strcmp(inotifytools_event_to_str_sep(
                        IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
                    "OPEN:ACCESS:MODIFY") ||
            !strcmp(inotifytools_event_to_str_sep(
                        IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
                    "ACCESS:OPEN:MODIFY") ||
            !strcmp(inotifytools_event_to_str_sep(
                        IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
                    "MODIFY:OPEN:ACCESS") ||
            !strcmp(inotifytools_event_to_str_sep(
                        IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
                    "ACCESS:MODIFY:OPEN") ||
            !strcmp(inotifytools_event_to_str_sep(
                        IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
                    "MODIFY:ACCESS:OPEN"),
        inotifytools_event_to_str_sep(IN_OPEN | IN_MODIFY | IN_ACCESS, ':'));
    EXIT
}

void str_to_event() {
    ENTER
    compare(inotifytools_str_to_event("open,modify,access"),
            IN_OPEN | IN_MODIFY | IN_ACCESS);
    compare(inotifytools_str_to_event(",open,modify,access"), 0);
    compare(inotifytools_str_to_event("open,modify,access,"), 0);
    compare(inotifytools_str_to_event("open,modify,,access,close"), 0);
    compare(inotifytools_str_to_event("open,mod,access,close"), -1);
    compare(inotifytools_str_to_event("mod"), -1);
    compare(inotifytools_str_to_event(","), 0);
    compare(inotifytools_str_to_event(",,"), 0);
    compare(inotifytools_str_to_event("open"), IN_OPEN);
    compare(inotifytools_str_to_event("close"), IN_CLOSE);
    compare(inotifytools_str_to_event(",close"), 0);
    compare(inotifytools_str_to_event(",,close"), 0);
    compare(inotifytools_str_to_event("close,"), 0);
    compare(inotifytools_str_to_event("close,,"), 0);
    EXIT
}

void str_to_event_sep() {
    ENTER
    compare(inotifytools_str_to_event_sep("open:modify:access", ':'),
            IN_OPEN | IN_MODIFY | IN_ACCESS);
    compare(inotifytools_str_to_event_sep("open,modify,access", ','),
            IN_OPEN | IN_MODIFY | IN_ACCESS);
    compare(inotifytools_str_to_event_sep(":open:modify:access", ':'), 0);
    compare(inotifytools_str_to_event_sep("open:modify:access:", ':'), 0);
    compare(inotifytools_str_to_event_sep("open:modify::access:close", ':'), 0);
    compare(inotifytools_str_to_event_sep("open:mod:access:close", ':'), -1);
    compare(inotifytools_str_to_event_sep("mod", ':'), -1);
    compare(inotifytools_str_to_event_sep(":", ':'), 0);
    compare(inotifytools_str_to_event_sep("::", ':'), 0);
    compare(inotifytools_str_to_event_sep("open", ':'), IN_OPEN);
    compare(inotifytools_str_to_event_sep("close", ':'), IN_CLOSE);
    compare(inotifytools_str_to_event_sep(":close", ':'), 0);
    compare(inotifytools_str_to_event_sep("::close", ':'), 0);
    compare(inotifytools_str_to_event_sep("close:", ':'), 0);
    compare(inotifytools_str_to_event_sep("close::", ':'), 0);
    compare(inotifytools_str_to_event_sep("open:modify:access", ','), -1);
    compare(inotifytools_str_to_event_sep("open:modify:access", 'o'), -1);
    EXIT
}

void basic_watch_info() {
    ENTER
    verify(inotifytools_initialize());
    verify(inotifytools_watch_file("/", IN_CLOSE));
    compare(inotifytools_wd_from_filename("/"), 1);
    compare(inotifytools_wd_from_filename("foobar"), -1);
    verify(!strcmp(inotifytools_filename_from_wd(1), "/"));
    verify(inotifytools_remove_watch_by_filename("/"));
    compare(inotifytools_wd_from_filename("/"), -1);
    compare(inotifytools_filename_from_wd(1), 0);
    verify(inotifytools_watch_file("/", IN_CLOSE));
    compare(inotifytools_wd_from_filename("/"), 2);
    compare(inotifytools_wd_from_filename("foobar"), -1);
    verify(!strcmp(inotifytools_filename_from_wd(2), "/"));
    verify(inotifytools_remove_watch_by_wd(2));
    compare(inotifytools_wd_from_filename("/"), -1);
    compare(inotifytools_filename_from_wd(2), 0);
    EXIT
}

void tst_inotifytools_snprintf() {
    ENTER
    verify((0 == mkdir(TEST_DIR, 0700)) || (EEXIST == errno));
    verify(inotifytools_initialize());
    verify(inotifytools_watch_file(TEST_DIR, IN_CLOSE));

#define BUFSZ 2048
#define RESET                                                                  \
    do {                                                                       \
        memset(buf, 0, BUFSZ);                                                 \
        memset(test_event, 0, sizeof(struct inotify_event));                   \
        test_event->wd = inotifytools_wd_from_filename(TEST_DIR "/");          \
        verify(test_event->wd >= 0);                                           \
        inotifytools_set_printf_timefmt(0);                                    \
    } while (0)

    char buf[BUFSZ];
    char event_buf[4096];
    struct inotify_event *test_event = (struct inotify_event *)event_buf;

    RESET;
    test_event->mask = IN_ACCESS;
    inotifytools_snprintf(buf, 1024, test_event, "Event %e %.e on %w %f %T");
    verify2(!strcmp(buf, "Event ACCESS ACCESS on " TEST_DIR "/  "), buf);

    RESET;
    test_event->mask = IN_ACCESS | IN_DELETE;
    inotifytools_snprintf(buf, 1024, test_event, "Event %e %.e on %w %f %T");
    verify2(
        !strcmp(buf, "Event ACCESS,DELETE ACCESS.DELETE on " TEST_DIR "/  ") ||
            !strcmp(buf,
                    "Event DELETE,ACCESS DELETE.ACCESS on " TEST_DIR "/  "),
        buf);

    RESET;
    test_event->mask = IN_MODIFY;
    inotifytools_snprintf(buf, 10, test_event, "Event %e %.e on %w %f %T");
    verify2(!strcmp(buf, "Event MODI"), buf);

    RESET;
    test_event->mask = IN_ACCESS;
    strcpy(test_event->name, "my_great_file");
    test_event->len = strlen(test_event->name) + 1;
    inotifytools_snprintf(buf, 1024, test_event, "Event %e %.e on %w %f %T");
    verify2(!strcmp(buf, "Event ACCESS ACCESS on " TEST_DIR "/ my_great_file "),
            buf);

    RESET;
    test_event->mask = IN_ACCESS;
    inotifytools_set_printf_timefmt("%D%% %H:%M");
    {
        char expected[1024];
        char timestr[512];
        time_t now = time(0);
        strftime(timestr, 512, "%D%% %H:%M", localtime(&now));
        snprintf(expected, 1024, "Event ACCESS ACCESS on %s/  %s", TEST_DIR,
                 timestr);
        inotifytools_snprintf(buf, 1024, test_event,
                              "Event %e %.e on %w %f %T");
        verify2(!strcmp(buf, expected), buf);
    }

#undef BUFSZ
    EXIT
}

void watch_limit() {
    ENTER
    verify((0 == mkdir(TEST_DIR, 0700)) || (EEXIST == errno));

    INFO("Warning, this test may take a while\n");
#define INNER_LIMIT 16000
#define OUTER_LIMIT 5

    verify(inotifytools_initialize());
    inotifytools_initialize_stats();

    for (int j = 0; j < OUTER_LIMIT; ++j) {
        char fn[1024];
        int max = 0;
        for (int i = 0; i < INNER_LIMIT; ++i) {
            snprintf(fn, 1023, "%s/%d", TEST_DIR, i);
            int fd = creat(fn, 0700);
            verify(-1 != fd);
            verify(0 == close(fd));
            int ret = inotifytools_watch_file(fn, IN_ALL_EVENTS);
            verify(ret || inotifytools_error() == ENOSPC);
            if (ret) {
                max = i + 1;
                int wd = inotifytools_wd_from_filename(fn);
                verify(wd > 0);
                verify(!strcmp(fn, inotifytools_filename_from_wd(wd)));
            }
        }

        compare(inotifytools_get_num_watches(), max);

        for (int i = 0; i < max; ++i) {
            snprintf(fn, 1023, "%s/%d", TEST_DIR, i);
            verify(inotifytools_remove_watch_by_filename(fn));
        }
    }
    EXIT
}

void cleanup() {
    compare(system("rm -rf " TEST_DIR), 0);
    inotifytools_cleanup();
}

int main() {
    tests_failed = 0;
    tests_succeeded = 0;

#ifdef HAVE_MCHECK_H
    char *mtrace_name = getenv("MALLOC_TRACE");
    if (mtrace_name) {
        printf("malloc trace might be written to %s\n", mtrace_name);
    } else {
        printf("If you want to do a malloc trace, set MALLOC_TRACE to "
               "a path for logging.\n");
    }
    mtrace();
#endif

    cleanup();

    event_to_str();
    cleanup();
    event_to_str_sep();
    cleanup();

    str_to_event();
    cleanup();
    str_to_event_sep();
    cleanup();

    basic_watch_info();
    cleanup();

    watch_limit();
    cleanup();

    tst_inotifytools_snprintf();
    cleanup();

    printf("Out of %d tests, %d succeeded and %d failed.\n",
           tests_failed + tests_succeeded, tests_succeeded, tests_failed);

    if (tests_failed)
        return -tests_failed;
}
