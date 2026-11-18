#include "inotifytools/inotify.h"
#include "inotifytools/inotifytools.h"

#if defined(__has_include)
#if __has_include(<mcheck.h>)
#define HAVE_MCHECK_H 1
#endif
#endif

#include <unistd.h>

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <initializer_list>
#include <string>

#ifdef HAVE_MCHECK_H
#include <mcheck.h>
#endif

#define verify(A)                                                            \
	if (!_verify((int)(A), #A, __FILE__, __LINE__, __PRETTY_FUNCTION__)) \
		return;
#define verify2(A, B)                                                       \
	if (!_verify((int)(A), B, __FILE__, __LINE__, __PRETTY_FUNCTION__)) \
		return;
#define compare(A, B)                                                       \
	if (!_compare((uintptr_t)(A), (int)(B), #A, #B, __FILE__, __LINE__, \
		      __PRETTY_FUNCTION__))                                 \
		return;

#define succeed()                                                       \
	do {                                                            \
		++tests_succeeded;                                      \
		/*fprintf(stderr, "%s:%d Test '%s' passed", file, line, \
		 * func);*/                                             \
		/*fprintf(stderr, "\n");*/                              \
	} while (0)

#define fail(...)                                                              \
	do {                                                                   \
		++tests_failed;                                                \
		fprintf(stderr, "%s:%d Test '%s' failed: ", file, line, func); \
		fprintf(stderr, __VA_ARGS__);                                  \
		fprintf(stderr, "\n");                                         \
		if (inotifytools_error() != 0) {                               \
			fprintf(stderr,                                        \
				"inotifytools_error() returns %d (%s)\n",      \
				inotifytools_error(),                          \
				strerror(inotifytools_error()));               \
		}                                                              \
	} while (0)

#define TEST_DIR "/tmp/inotifytools_test"

#define INFO(...)                                    \
	do {                                         \
		printf("%s: ", __PRETTY_FUNCTION__); \
		printf(__VA_ARGS__);                 \
	} while (0)

#define ENTER INFO("test begin\n");
#define EXIT INFO("test end\n");

static int tests_failed;
static int tests_succeeded;

int _verify(int cond,
	    char const* teststr,
	    char const* file,
	    int line,
	    char const* func) {
	if (cond) {
		succeed();
		return cond;
	}

	fail("Verification failed (%s)", teststr);
	return cond;
}

int _compare(int value,
	     int expval,
	     char const* act_str,
	     char const* exp_str,
	     char const* file,
	     int line,
	     char const* func) {
	if (value == expval) {
		succeed();
		return 1;
	}

	fail("Actual (%s): %d, Expected (%s): %d", act_str, value, exp_str,
	     expval);
	return 0;
}

void event_to_str() {
	ENTER
	verify2(
	    !strcmp(inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "OPEN,MODIFY,ACCESS") ||
		!strcmp(
		    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "OPEN,ACCESS,MODIFY") ||
		!strcmp(
		    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "ACCESS,OPEN,MODIFY") ||
		!strcmp(
		    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "MODIFY,OPEN,ACCESS") ||
		!strcmp(
		    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "ACCESS,MODIFY,OPEN") ||
		!strcmp(
		    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS),
		    "MODIFY,ACCESS,OPEN"),
	    inotifytools_event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS));
	EXIT
}

void event_to_str_sep() {
	ENTER
	verify2(!strcmp(inotifytools_event_to_str_sep(
			    IN_OPEN | IN_MODIFY | IN_ACCESS, ':'),
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
		inotifytools_event_to_str_sep(IN_OPEN | IN_MODIFY | IN_ACCESS,
					      ':'));
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
	compare(inotifytools_str_to_event_sep("open:modify::access:close", ':'),
		0);
	compare(inotifytools_str_to_event_sep("open:mod:access:close", ':'),
		-1);
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
	compare(*inotifytools_filename_from_wd(1), 0);
	verify(inotifytools_watch_file("/", IN_CLOSE));
	compare(inotifytools_wd_from_filename("/"), 2);
	compare(inotifytools_wd_from_filename("foobar"), -1);
	verify(!strcmp(inotifytools_filename_from_wd(2), "/"));
	verify(inotifytools_remove_watch_by_wd(2));
	compare(inotifytools_wd_from_filename("/"), -1);
	compare(*inotifytools_filename_from_wd(2), 0);
	EXIT
}

void tst_inotifytools_snprintf() {
	ENTER
	verify((0 == mkdir(TEST_DIR, 0700)) || (EEXIST == errno));
	verify(inotifytools_initialize());
	verify(inotifytools_watch_file(TEST_DIR, IN_CLOSE));

#define RESET                                                                 \
	do {                                                                  \
		memset(out.buf, 0, MAX_STRLEN);                               \
		memset(test_event, 0, sizeof(struct inotify_event));          \
		test_event->wd = inotifytools_wd_from_filename(TEST_DIR "/"); \
		verify(test_event->wd >= 0);                                  \
		inotifytools_clear_timefmt();                                 \
	} while (0)

	struct nstring out;
	char event_buf[4096];
	struct inotify_event* test_event = (struct inotify_event*)event_buf;

	RESET;
	test_event->mask = IN_ACCESS;
	inotifytools_snprintf(&out, MAX_STRLEN, test_event,
			      "Event %e %.e on %w %f %T");
	verify2(!strcmp(out.buf, "Event ACCESS ACCESS on " TEST_DIR "/  "),
		out.buf);

	RESET;
	test_event->mask = IN_ACCESS | IN_DELETE;
	inotifytools_snprintf(&out, MAX_STRLEN, test_event,
			      "Event %e %.e on %w %f %T");
	verify2(
	    !strcmp(out.buf,
		    "Event ACCESS,DELETE ACCESS.DELETE on " TEST_DIR "/  ") ||
		!strcmp(out.buf,
			"Event DELETE,ACCESS DELETE.ACCESS on " TEST_DIR "/  "),
	    out.buf);

	RESET;
	test_event->mask = IN_MODIFY;
	inotifytools_snprintf(&out, 10, test_event, "Event %e %.e on %w %f %T");
	verify2(!strcmp(out.buf, "Event MODI"), out.buf);

	RESET;
	test_event->mask = IN_ACCESS;
	strncpy(test_event->name, "my_great_file", 14);
	test_event->len = strlen(test_event->name) + 1;
	inotifytools_snprintf(&out, MAX_STRLEN, test_event,
			      "Event %e %.e on %w %f %T");
	verify2(!strcmp(out.buf,
			"Event ACCESS ACCESS on " TEST_DIR "/ my_great_file "),
		out.buf);

	RESET;
	test_event->mask = IN_ACCESS;
	inotifytools_set_printf_timefmt("%D%% %H:%M");
	{
		char expected[MAX_STRLEN];
		char timestr[512];
		time_t now = time(0);
		strftime(timestr, 512, "%D%% %H:%M", localtime(&now));
		snprintf(expected, MAX_STRLEN, "Event ACCESS ACCESS on %s/  %s",
			 TEST_DIR, timestr);
		inotifytools_snprintf(&out, MAX_STRLEN, test_event,
				      "Event %e %.e on %w %f %T");
		verify2(!strcmp(out.buf, expected), out.buf);
	}

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
				verify(!strcmp(
				    fn, inotifytools_filename_from_wd(wd)));
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

struct batch_files {
	char directory[64] = "/tmp/inotifytools_batch_XXXXXX";

	std::string path(char const* name) const {
		return std::string(directory) + "/" + name;
	}

	bool create(char const* name) {
		int fd = creat(path(name).c_str(), 0600);
		return fd >= 0 && close(fd) == 0;
	}

	~batch_files() {
		for (char const* name : {"first", "second"}) {
			unlink(path(name).c_str());
		}
		rmdir(directory);
	}
};

pid_t reap(pid_t pid, int* status) {
	pid_t waited;
	do {
		waited = waitpid(pid, status, 0);
	} while (waited < 0 && errno == EINTR);
	return waited;
}

void partial_batch_timeout(long timeout) {
	ENTER
	batch_files files;
	verify(mkdtemp(files.directory) != NULL);
	verify(files.create("first"));
	verify(files.create("second"));
	std::string first = files.path("first"), second = files.path("second");
	verify(inotifytools_initialize());
	verify(inotifytools_watch_file(first.c_str(), IN_ATTRIB));
	verify(inotifytools_watch_file(second.c_str(), IN_ATTRIB));
	int first_wd = inotifytools_wd_from_filename(first.c_str());
	int second_wd = inotifytools_wd_from_filename(second.c_str());
	// File watches produce unnamed events: exactly one event header each,
	// independent of the platform's padding for directory event names.
	verify(chmod(first.c_str(), 0400) == 0);

	// Release an old implementation which ignores the batch timeout, so a
	// regression fails instead of hanging the entire C ABI suite.
	pid_t producer = fork();
	verify(producer >= 0);
	if (producer == 0) {
		struct timespec delay = {timeout + 1, 0};
		while (nanosleep(&delay, &delay) < 0) {
			if (errno != EINTR)
				_exit(1);
		}
		_exit(chmod(second.c_str(), 0400) == 0 ? 0 : 1);
	}

	bool timed_out = inotifytools_next_events(timeout, 2) == NULL;
	int error = inotifytools_error();
	int status = 0;
	pid_t waited = reap(producer, &status);
	verify(waited == producer && WIFEXITED(status) &&
	       WEXITSTATUS(status) == 0);
	verify(timed_out);
	compare(error, 0);

	// The timeout must not consume the partial batch. After the producer
	// exits both events are queued; subsequent calls also cover the buffer.
	struct inotify_event* event = inotifytools_next_events(0, 2);
	verify(event && (event->mask & IN_ATTRIB));
	compare(event->wd, first_wd);
	event = inotifytools_next_events(0, 4096);
	verify(event && (event->mask & IN_ATTRIB));
	compare(event->wd, second_wd);
	verify(inotifytools_next_events(0, 1) == NULL);
	compare(inotifytools_error(), 0);
	EXIT
}

void single_event_blocks() {
	ENTER
	batch_files files;
	verify(mkdtemp(files.directory) != NULL);
	verify(files.create("first"));
	std::string first = files.path("first");
	verify(inotifytools_initialize());
	verify(inotifytools_watch_file(first.c_str(), IN_ATTRIB));
	int wd = inotifytools_wd_from_filename(first.c_str());
	int messages[2];
	verify(pipe(messages) == 0);
	pid_t waiter = fork();
	if (waiter < 0) {
		close(messages[0]);
		close(messages[1]);
	}
	verify(waiter >= 0);
	if (waiter == 0) {
		close(messages[0]);
		// Bound a regression which never wakes after the event, too.
		alarm(5);
		char message = 'R';
		if (write(messages[1], &message, 1) != 1)
			_exit(1);
		struct inotify_event* event = inotifytools_next_event(0);
		if (!event || event->wd != wd || !(event->mask & IN_ATTRIB) ||
		    inotifytools_error() != 0)
			_exit(2);
		message = 'E';
		_exit(write(messages[1], &message, 1) == 1 ? 0 : 3);
	}
	close(messages[1]);
	struct pollfd result = {messages[0], POLLIN, 0};
	char message = 0;
	bool started = poll(&result, 1, 3000) > 0 &&
		       read(messages[0], &message, 1) == 1 && message == 'R';
	// No filesystem event exists yet: a zero-timeout single-event call
	// must remain blocked, unlike next_events(0, num_events)'s poll.
	bool blocked = started && poll(&result, 1, 200) == 0;
	bool produced = blocked && chmod(first.c_str(), 0400) == 0;
	bool delivered = produced && poll(&result, 1, 3000) > 0 &&
			 read(messages[0], &message, 1) == 1 && message == 'E';
	close(messages[0]);
	// Always reap the waiter before assertions can return from the test.
	if (!delivered)
		kill(waiter, SIGKILL);
	int status = 0;
	pid_t waited = reap(waiter, &status);
	verify(started);
	verify(blocked);
	verify(produced);
	verify(delivered);
	verify(waited == waiter && WIFEXITED(status) &&
	       WEXITSTATUS(status) == 0);
	EXIT
}

void single_event_idle_timeout() {
	ENTER
	batch_files files;
	verify(mkdtemp(files.directory) != NULL);
	verify(files.create("first"));
	std::string first = files.path("first");
	verify(inotifytools_initialize());
	verify(inotifytools_watch_file(first.c_str(), IN_ATTRIB));
	int wd = inotifytools_wd_from_filename(first.c_str());
	pid_t waiter = fork();
	verify(waiter >= 0);
	if (waiter == 0) {
		// An implementation which ignores the positive timeout must
		// fail instead of hanging the rest of the shared/static C ABI
		// suite, even if its launcher blocked or ignored SIGALRM.
		sigset_t alarms;
		if (signal(SIGALRM, SIG_DFL) == SIG_ERR ||
		    sigemptyset(&alarms) != 0 ||
		    sigaddset(&alarms, SIGALRM) != 0 ||
		    sigprocmask(SIG_UNBLOCK, &alarms, NULL) != 0)
			_exit(1);
		alarm(5);
		struct timespec start, finish;
		if (clock_gettime(CLOCK_MONOTONIC, &start) != 0)
			_exit(1);
		struct inotify_event* event = inotifytools_next_event(1);
		int error = inotifytools_error();
		if (clock_gettime(CLOCK_MONOTONIC, &finish) != 0)
			_exit(1);
		long long elapsed_ns =
		    (finish.tv_sec - start.tv_sec) * 1000000000LL +
		    (finish.tv_nsec - start.tv_nsec);
		// Allow scheduling overhead, but reject a materially late
		// timeout.
		if (event || error != 0 || elapsed_ns < 1000000000LL ||
		    elapsed_ns >= 3000000000LL)
			_exit(2);
		// The expired wait must leave the watch usable for the next
		// event.
		if (chmod(first.c_str(), 0400) != 0)
			_exit(3);
		event = inotifytools_next_event(1);
		if (!event || event->wd != wd || !(event->mask & IN_ATTRIB) ||
		    inotifytools_error() != 0)
			_exit(4);
		_exit(inotifytools_next_events(0, 1) == NULL &&
			      inotifytools_error() == 0
			  ? 0
			  : 5);
	}
	int status = 0;
	verify(reap(waiter, &status) == waiter);
	verify(WIFEXITED(status));
	compare(WEXITSTATUS(status), 0);
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
	char* mtrace_name = getenv("MALLOC_TRACE");
	if (mtrace_name) {
		printf("malloc trace might be written to %s\n", mtrace_name);
	} else {
		printf(
		    "If you want to do a malloc trace, set MALLOC_TRACE to "
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

	single_event_blocks();
	cleanup();
	single_event_idle_timeout();
	cleanup();

	partial_batch_timeout(1);
	cleanup();
	partial_batch_timeout(0);
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
