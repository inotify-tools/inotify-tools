//! inotifywait - wait for changes to files using inotify (or fanotify).

mod common;

use std::ffi::CString;

use common::*;
use inotifytools::cio::{self, cstring, errno, strerror};
use inotifytools::consts::*;
use inotifytools::{cat, ceprint, cprint, Event, Inotifytools, WatchScope, MAX_STRLEN};
use libc::{c_int, c_long};

#[derive(Default)]
struct Opts {
    events: i32,
    monitor: bool,
    quiet: i32,
    timeout: c_long,
    recursive: i32,
    csv: bool,
    daemon: bool,
    syslog: bool,
    no_dereference: bool,
    format: Option<Vec<u8>>,
    timefmt: Option<Vec<u8>>,
    fromfile: Option<Vec<u8>>,
    outfile: Option<Vec<u8>>,
    filters: Filters,
    no_newline: bool,
    fanotify: bool,
    scope: Option<WatchScope>,
}

/// CSV-escape the first `len` bytes of `string`.  (Whether escaping is
/// needed is decided on the whole string, like the original.)
fn csv_escape_len(string: &[u8], len: usize) -> Vec<u8> {
    if len == 0 || len > MAX_STRLEN || len > string.len() {
        return Vec::new();
    }

    // May not need escaping
    if !string.contains(&b'"')
        && !string.contains(&b',')
        && !string.contains(&b'\n')
        && string[0] != b' '
        && string[len - 1] != b' '
    {
        return string[..len].to_vec();
    }

    // OK, so now we _do_ need escaping.
    let mut csv = vec![b'"'];
    for &c in &string[..len] {
        if c == b'"' {
            csv.push(b'"');
        }
        csv.push(c);
    }
    csv.push(b'"');
    csv
}

fn csv_escape(string: &[u8]) -> Vec<u8> {
    csv_escape_len(string, string.len())
}

fn validate_format(lib: &mut Inotifytools, fmt: &[u8]) {
    // Make a fake event
    let event = Event { wd: 0, mask: IN_ALL_EVENTS as u32, cookie: 0, len: 3, name: Vec::new() };
    let devnull = unsafe { libc::fopen(b"/dev/null\0".as_ptr().cast(), b"a\0".as_ptr().cast()) };
    if devnull.is_null() {
        ceprint!("Couldn't open /dev/null: ", strerror(errno()), "\n");
        return;
    }

    if unsafe { lib.fprintf_to(devnull, &event, Some(fmt)) } == -1 {
        ceprint!("Something is wrong with your format string.\n");
        unsafe { libc::fclose(devnull) };
        std::process::exit(EXIT_FAILURE);
    }

    unsafe { libc::fclose(devnull) };
}

fn output_event_csv(lib: &mut Inotifytools, event: &Event) {
    let path = lib.filename_from_event(event);
    let filename = csv_escape_len(&path.filename, path.dirnamelen);
    if !filename.is_empty() {
        cprint!(filename, ",");
    }
    cprint!(csv_escape(&lib.event_to_str(event.mask as i32)), ",");
    cprint!(csv_escape(&path.eventname));
    cprint!("\n");
}

fn output_error(syslog: bool, msg: Vec<u8>) {
    if syslog {
        let c = cstring(&msg);
        unsafe { libc::syslog(libc::LOG_INFO, b"%s\0".as_ptr().cast(), c.as_ptr()) };
    } else {
        cio::err(&msg);
    }
}

fn output_event(lib: &mut Inotifytools, o: &Opts, event: &Event) {
    if o.csv {
        output_event_csv(lib, event);
    } else if let Some(f) = &o.format {
        lib.printf(event, Some(f));
    } else {
        lib.printf(event, Some(b"%w %,e %f\n"));
    }
}

/// Redirect `fd` to a newly opened file (like the original's
/// open()/dup2()/close()).
fn redirect(path: &CString, flags: c_int, mode: libc::c_uint, target: c_int) -> bool {
    let fd = unsafe { libc::open(path.as_ptr(), flags, mode) };
    if fd < 0 {
        return false;
    }
    if fd != target {
        unsafe {
            libc::dup2(fd, target);
            libc::close(fd);
        }
    }
    true
}

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let mut o = Opts { timeout: BLOCKING_TIMEOUT, ..Default::default() };

    let mut g = GetOpt::new(OPT_STRING, LONG_OPTS);
    if g.argc() > 0 && invoked_as_fsnotify(g.argv0().as_deref()) {
        // Default to fanotify for the fsnotify* tools.
        o.fanotify = true;
    }

    // Parse commandline options, aborting if something goes wrong
    let args = match parse_opts(&mut g, &mut o) {
        Some(v) => v,
        None => {
            return EXIT_FAILURE;
        }
    };
    let scope = o.scope.unwrap_or_default();

    let mut lib = Inotifytools::new();
    if !lib.init(o.fanotify, scope, (o.quiet == 0) as c_int) {
        warn_inotify_init_error(&lib, o.fanotify);
        return EXIT_FAILURE;
    }

    if let Some(t) = &o.timefmt {
        lib.set_printf_timefmt(t);
    }
    if !o.filters.install(&mut lib, o.recursive) {
        return EXIT_FAILURE;
    }

    if let Some(f) = o.format.clone() {
        validate_format(&mut lib, &f);
    }

    // Attempt to watch file
    // If events is still 0, make it all events.
    let mut events = o.events;
    if events == 0 {
        events = scope.default_events();
    }

    let orig_events = events;
    if o.monitor && o.recursive != 0 {
        events |= IN_CREATE | IN_MOVED_TO | IN_MOVED_FROM;
    }

    if o.no_dereference {
        events |= IN_DONT_FOLLOW;
    }

    if o.fanotify {
        events |= IN_ISDIR;
    }

    let list = match construct_path_list(&args, o.fromfile.as_deref()) {
        Some(v) => v,
        None => {
            return EXIT_FAILURE;
        }
    };

    if list.watch_files.is_empty() {
        ceprint!("No files specified to watch!\n");
        return EXIT_FAILURE;
    }

    // Daemonize - BSD double-fork approach
    if o.daemon {
        let outfile = o.outfile.as_deref().unwrap_or_default();
        let coutfile = cstring(outfile);
        // Absolute path for outfile before entering the child.
        let mut buf = vec![0u8; libc::PATH_MAX as usize + 1];
        if unsafe { libc::realpath(coutfile.as_ptr(), buf.as_mut_ptr().cast()) }.is_null() {
            ceprint!(strerror(errno()), ": ", outfile, "\n");
            return EXIT_FAILURE;
        }
        let logfile = cstring(&buf);

        if unsafe { inotifytools::sys::daemon(0, 0) } != 0 {
            ceprint!("Failed to daemonize!\n");
            return EXIT_FAILURE;
        }

        // Redirect stdin from /dev/null
        let devnull = CString::new("/dev/null").unwrap();
        redirect(&devnull, libc::O_RDONLY, 0, libc::STDIN_FILENO);

        // Redirect stdout to a file
        if !redirect(
            &logfile,
            libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
            0o600,
            libc::STDOUT_FILENO,
        ) {
            ceprint!("Failed to open output file ", logfile, "\n");
            return EXIT_FAILURE;
        }

        // Redirect stderr to /dev/null
        redirect(&devnull, libc::O_WRONLY, 0, libc::STDERR_FILENO);
    } else if let Some(outfile) = &o.outfile {
        // Redirect stdout to a file if specified
        let c = cstring(outfile);
        if !redirect(
            &c,
            libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
            0o600,
            libc::STDOUT_FILENO,
        ) {
            ceprint!("Failed to open output file ", outfile, "\n");
            return EXIT_FAILURE;
        }
    }

    let sysl = o.syslog;
    if sysl {
        unsafe {
            libc::openlog(
                b"inotifywait\0".as_ptr().cast(),
                libc::LOG_CONS | libc::LOG_PID | libc::LOG_NDELAY,
                libc::LOG_DAEMON,
            )
        };
    }

    if o.quiet == 0 {
        if scope != WatchScope::Inode {
            output_error(sysl, cat!("Setting up ", scope.name(), " watches.\n"));
        } else if o.recursive != 0 {
            output_error(
                sysl,
                cat!("Setting up watches.  Beware: since -r was given, this may take a while!\n"),
            );
        } else {
            output_error(sysl, cat!("Setting up watches.\n"));
        }
    }

    let recursive = o.recursive != 0;
    let mut out = |m| output_error(sysl, m);
    if !watch_list(
        &mut lib,
        &list,
        events,
        recursive,
        scope,
        o.fanotify,
        "Couldn't watch ",
        &mut out,
        &mut |_| {},
        &mut |_| {},
    ) {
        return EXIT_FAILURE;
    }

    if o.quiet == 0 {
        output_error(sysl, cat!("Watches established.\n"));
    }
    if o.timeout < 0 {
        // Used to test filesystem support for inotify/fanotify
        ceprint!("Negative timeout specified - abort!\n");
        return EXIT_FAILURE;
    }

    // Now wait till we get event
    let mut moves = MoveTracker::default();
    let has_include = o.filters.has_include();

    let last_event = loop {
        let event = match lib.next_event(o.timeout) {
            Some(v) => v,
            None => {
                if lib.error() == 0 {
                    return EXIT_TIMEOUT;
                }
                output_error(sysl, cat!(strerror(lib.error()), "\n"));
                return EXIT_FAILURE;
            }
        };

        if o.quiet < 2 && (event.mask as i32 & orig_events) != 0 {
            // With an include filter, directory events are not shown.
            if !has_include || event.mask as i32 & IN_ISDIR == 0 {
                output_event(&mut lib, &o, &event);
            }
        }

        // TODO: replace filename of renamed filesystem watch entries
        if scope != WatchScope::Inode {
            if !o.monitor {
                break event;
            }
            continue;
        }

        let recursive = o.monitor && o.recursive != 0;
        moves.handle(&mut lib, &event, recursive, events, o.quiet == 0, &mut out);

        cio::flush_all();

        if !o.monitor {
            break event;
        }
    };

    // If we weren't trying to listen for this event...
    if events & last_event.mask as i32 == 0 {
        // ...then most likely something bad happened, like IGNORE etc.
        return EXIT_FAILURE;
    }

    EXIT_SUCCESS
}

const OPT_STRING: &str = "mrhcdsPqt:fo:e:IFMS";

const LONG_OPTS: &[LongOpt] = &[
    ("help", false, b'h'),
    ("event", true, b'e'),
    ("monitor", false, b'm'),
    ("quiet", false, b'q'),
    ("timeout", true, b't'),
    ("filename", false, b'f'),
    ("recursive", false, b'r'),
    ("inotify", false, b'I'),
    ("fanotify", false, b'F'),
    ("filesystem", false, b'S'),
    ("mount", false, b'M'),
    ("csv", false, b'c'),
    ("daemon", false, b'd'),
    ("syslog", false, b's'),
    ("no-dereference", false, b'P'),
    ("format", true, b'n'),
    ("no-newline", false, b'0'),
    ("timefmt", true, b'i'),
    ("fromfile", true, b'z'),
    ("outfile", true, b'o'),
    ("exclude", true, b'a'),
    ("excludei", true, b'b'),
    ("include", true, b'j'),
    ("includei", true, b'k'),
];

/// Parse the command line.  Returns the remaining (file) arguments, or
/// `None` if the program should exit with failure.
fn parse_opts(g: &mut GetOpt, o: &mut Opts) -> Option<Vec<Vec<u8>>> {
    // How many times --exclude / --excludei have been specified
    let mut exclude_count = 0u32;
    let mut excludei_count = 0u32;

    let regex_warning = "only the last option will be taken into consideration.\n";

    let arg = |g: &GetOpt| g.optarg().map(|a| a.to_bytes().to_vec());

    let mut curr_opt = g.next();
    while curr_opt != OPT_ERR && curr_opt != OPT_END {
        match curr_opt as u8 {
            b'h' => {
                print_help(&help_name(g));
                return None;
            }
            b'm' => o.monitor = true,
            b'q' => o.quiet += 1,
            b'r' => o.recursive += 1,
            b'I' | b'S' | b'M' => {
                if !set_watch_scope(&mut o.scope, &mut o.fanotify, curr_opt as u8) {
                    return None;
                }
            }
            b'F' => o.fanotify = true,
            b'c' => o.csv = true,
            b'd' => {
                o.daemon = true;
                o.monitor = true;
                o.syslog = true;
            }
            b's' => o.syslog = true,
            b'P' => o.no_dereference = true,
            b'f' => {
                ceprint!(
                    "The '--filename' option no longer exists.  The option it enabled in earlier\n",
                    "versions of inotifywait is now turned on by default.\n"
                );
                return None;
            }
            b'n' => o.format = arg(g),
            b'0' => o.no_newline = true,
            b'i' => o.timefmt = arg(g),
            b'a' => {
                o.filters.exc = arg(g);
                exclude_count += 1;
            }
            b'b' => {
                o.filters.exci = arg(g);
                excludei_count += 1;
            }
            b'j' => o.filters.inc = arg(g),
            b'k' => o.filters.inci = arg(g),
            b'z' => {
                if o.fromfile.is_some() {
                    ceprint!("Multiple --fromfile options given.\n");
                    return None;
                }
                o.fromfile = arg(g);
            }
            b'o' => {
                if o.outfile.is_some() {
                    ceprint!("Multiple --outfile options given.\n");
                    return None;
                }
                o.outfile = arg(g);
            }
            b't' => {
                if !is_timeout_option_valid(&mut o.timeout, g.optarg()) {
                    return None;
                }
            }
            b'e' => o.events |= parse_event(g.optarg())?,
            _ => {}
        }

        curr_opt = g.next();
    }

    if let Some(f) = &mut o.format {
        if !o.no_newline {
            f.push(b'\n');
        }
    }

    if !o.filters.validate() {
        return None;
    }

    if o.format.is_some() && o.csv {
        ceprint!("-c and --format cannot both be specified.\n");
        return None;
    }

    if o.format.is_none() && o.no_newline {
        ceprint!("--no-newline cannot be specified without --format.\n");
        return None;
    }

    if o.format.is_none() && o.timefmt.is_some() {
        ceprint!("--timefmt cannot be specified without --format.\n");
        return None;
    }

    if let Some(f) = &o.format {
        if f.windows(2).any(|w| w == b"%T") && o.timefmt.is_none() {
            ceprint!("%T is in --format string, but --timefmt was not specified.\n");
            return None;
        }
    }

    if o.daemon && o.outfile.is_none() {
        ceprint!("-o must be specified with -d.\n");
        return None;
    }

    if exclude_count > 1 {
        ceprint!("--exclude: ", regex_warning);
    }

    if excludei_count > 1 {
        ceprint!("--excludei: ", regex_warning);
    }

    let rest = g.remaining();

    // If ? returned, invalid option
    if curr_opt == OPT_ERR {
        return None;
    }
    Some(rest)
}

fn print_help(tool_name: &[u8]) {
    let t = tool_name;
    cprint!(t, " ", package_version(), "\n");
    cprint!("Wait for a particular event on a file or set of files.\n");
    cprint!("Usage: ", t, " [ options ] file1 [ file2 ] [ file3 ] [ ... ]\n");
    cprint!("Options:\n");
    cprint!("\t-h|--help     \tShow this help text.\n");
    cprint!("\t@<file>       \tExclude the specified file from being watched.\n");
    cprint!(
        "\t--exclude <pattern>\n",
        "\t              \tExclude all events on files matching the\n",
        "\t              \textended regular expression <pattern>.\n",
        "\t              \tOnly the last --exclude option will be\n",
        "\t              \ttaken into consideration.\n"
    );
    cprint!("\t--excludei <pattern>\n", "\t              \tLike --exclude but case insensitive.\n");
    cprint!(
        "\t--include <pattern>\n",
        "\t              \tExclude all events on files except the ones\n",
        "\t              \tmatching the extended regular expression\n",
        "\t              \t<pattern>.\n"
    );
    cprint!("\t--includei <pattern>\n", "\t              \tLike --include but case insensitive.\n");
    cprint!(
        "\t-m|--monitor  \tKeep listening for events forever or until --timeout expires.\n",
        "\t              \tWithout this option, ",
        t,
        " will exit after one event is received.\n"
    );
    cprint!(
        "\t-d|--daemon   \tSame as --monitor, except run in the background\n",
        "\t              \tlogging events to a file specified by --outfile.\n",
        "\t              \tImplies --syslog.\n"
    );
    cprint!("\t-P|--no-dereference\n", "\t              \tDo not follow symlinks.\n");
    cprint!("\t-r|--recursive\tWatch directories recursively.\n");
    cprint!("\t-I|--inotify\tWatch with inotify.\n");
    cprint!("\t-F|--fanotify\tWatch with fanotify.\n");
    cprint!("\t-S|--filesystem\tWatch entire filesystem with fanotify.\n");
    cprint!("\t-M|--mount\tWatch entire mount with fanotify.\n");
    cprint!(
        "\t--fromfile <file>\n",
        "\t              \tRead files to watch from <file> or `-' for stdin.\n"
    );
    cprint!(
        "\t-o|--outfile <file>\n",
        "\t              \tPrint events to <file> rather than stdout.\n"
    );
    cprint!("\t-s|--syslog   \tSend errors to syslog rather than stderr.\n");
    cprint!("\t-q|--quiet    \tPrint less (only print events).\n");
    cprint!("\t-qq           \tPrint nothing (not even events).\n");
    cprint!(
        "\t--format <fmt>\tPrint using a specified printf-like format\n",
        "\t              \tstring; read the man page for more details.\n"
    );
    cprint!(
        "\t--no-newline  \tDon't print newline symbol after\n",
        "\t              \t--format string.\n"
    );
    cprint!(
        "\t--timefmt <fmt>\tstrftime-compatible format string for use with\n",
        "\t              \t%T in --format string.\n"
    );
    cprint!("\t-c|--csv      \tPrint events in CSV format.\n");
    cprint!(
        "\t-t|--timeout <seconds>\n",
        "\t              \tWhen listening for a single event, time out after\n",
        "\t              \twaiting for an event for <seconds> seconds.\n",
        "\t              \tIf <seconds> is zero, ",
        t,
        " will never time out.\n"
    );
    cprint!(
        "\t-e|--event <event1> [ -e|--event <event2> ... ]\n",
        "\t\tListen for specific event(s).  If omitted, all events are \n",
        "\t\tlistened for.\n\n"
    );
    cprint!("Exit status:\n");
    cprint!("\t", EXIT_SUCCESS, "  -  An event you asked to watch for was received.\n");
    cprint!("\t", EXIT_FAILURE, "  -  An event you did not ask to watch for was received\n");
    cprint!("\t      (usually delete_self or unmount), or some error occurred.\n");
    cprint!("\t", EXIT_TIMEOUT, "  -  The --timeout option was given and no events occurred\n");
    cprint!("\t      in the specified interval of time.\n\n");
    cprint!("Events:\n");
    print_event_descriptions();
}
