//! inotifywatch - gather filesystem access statistics using inotify (or
//! fanotify).

mod common;

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use common::*;
use inotifytools::cio::{self, strerror};
use inotifytools::consts::*;
use inotifytools::{ceprint, cprint, Inotifytools, WatchScope, WatchStats};
use libc::{c_int, c_long};

static DONE: AtomicBool = AtomicBool::new(false);
static PRINT_NOW: AtomicBool = AtomicBool::new(false);
static TIMES_CALLED: AtomicI32 = AtomicI32::new(0);

fn raw_stderr(msg: &[u8]) {
    unsafe {
        libc::write(libc::STDERR_FILENO, msg.as_ptr().cast(), msg.len());
    }
}

extern "C" fn handle_impatient_user(_signal: c_int) {
    if TIMES_CALLED.load(Ordering::SeqCst) != 0 {
        raw_stderr(
            b"No statistics collected, asked to abort before all watches could be established.\n",
        );
        unsafe { libc::_exit(1) };
    }

    raw_stderr(
        b"No statistics have been collected because I haven't finished establishing\n\
inotify watches yet.  If you are sure you want me to exit, interrupt me again.\n",
    );
    TIMES_CALLED.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn handle_signal(_signal: c_int) {
    DONE.store(true, Ordering::SeqCst);
}

extern "C" fn print_info_now(_signal: c_int) {
    PRINT_NOW.store(true, Ordering::SeqCst);
}

fn set_signal(sig: c_int, handler: extern "C" fn(c_int)) {
    unsafe {
        libc::signal(sig, handler as libc::sighandler_t);
    }
}

#[derive(Default)]
struct Opts {
    events: i32,
    timeout: c_long,
    verbose: i32,
    zero: i32,
    sort: i32,
    recursive: i32,
    no_dereference: i32,
    fromfile: Option<Vec<u8>>,
    filters: Filters,
    fanotify: bool,
    scope: Option<WatchScope>,
}

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let mut o = Opts { timeout: BLOCKING_TIMEOUT, sort: -1, ..Default::default() };

    let mut g = GetOpt::new(OPT_STRING, LONG_OPTS);
    if g.argc() > 0 && invoked_as_fsnotify(g.argv0().as_deref()) {
        // Default to fanotify for the fsnotify* tools.
        o.fanotify = true;
    }

    set_signal(libc::SIGINT, handle_impatient_user);

    // Parse commandline options, aborting if something goes wrong
    let args = match parse_opts(&mut g, &mut o) {
        Some(v) => v,
        None => {
            return EXIT_FAILURE;
        }
    };
    let scope = o.scope.unwrap_or_default();

    let mut lib = Inotifytools::new();
    if !o.filters.install(&mut lib, o.recursive) {
        return EXIT_FAILURE;
    }

    if !lib.init(o.fanotify, scope, o.verbose) {
        warn_inotify_init_error(&lib, o.fanotify);
        return EXIT_FAILURE;
    }

    // Attempt to watch file
    // If events is still 0, make it all events.
    if o.events == 0 {
        o.events = scope.default_events();
    }
    let mut events = o.events;
    if o.no_dereference != 0 {
        events |= IN_DONT_FOLLOW;
    }

    if o.fanotify {
        events |= IN_ISDIR;
    }
    o.events = events;

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

    ceprint!("Establishing watches...\n");
    let (recursive, verbose) = (o.recursive != 0, o.verbose != 0);
    let mut before = |f: &[u8]| {
        if scope != WatchScope::Inode {
            ceprint!("Setting up ", scope.name(), " watch on ", f, "\n");
        } else if recursive && verbose {
            ceprint!("Setting up watch(es) on ", f, "\n");
        }
    };
    let mut after = |f: &[u8]| {
        if recursive && verbose {
            ceprint!("OK, ", f, " is now being watched.\n");
        }
    };
    let mut out = |m: Vec<u8>| cio::err(&m);
    if !watch_list(
        &mut lib,
        &list,
        events,
        recursive,
        scope,
        o.fanotify,
        "Failed to watch ",
        &mut out,
        &mut before,
        &mut after,
    ) {
        return EXIT_FAILURE;
    }
    let num_watches = lib.get_num_watches() as u32;

    if o.verbose != 0 {
        ceprint!("Total of ", num_watches, " watches.\n");
    }
    ceprint!("Finished establishing watches, now collecting statistics.\n");

    if o.timeout < 0 {
        // Used to test filesystem support for inotify/fanotify
        ceprint!("Negative timeout specified - abort!\n");
        return EXIT_FAILURE;
    }
    if o.timeout != 0 && o.verbose != 0 {
        ceprint!("Will listen for events for ", o.timeout as libc::c_ulong, " seconds.\n");
    }

    set_signal(libc::SIGINT, handle_signal);
    set_signal(libc::SIGHUP, handle_signal);
    set_signal(libc::SIGTERM, handle_signal);
    if o.timeout != 0 {
        set_signal(libc::SIGALRM, handle_signal);
        unsafe { libc::alarm(o.timeout as libc::c_uint) };
    } else {
        unsafe { libc::alarm(libc::c_uint::MAX) };
    }

    set_signal(libc::SIGUSR1, print_info_now);

    lib.initialize_stats();
    // Now wait till we get event
    let mut moves = MoveTracker::default();

    loop {
        check_print_now(&mut lib, &o);
        let event = lib.next_event(BLOCKING_TIMEOUT);
        check_print_now(&mut lib, &o);
        match event {
            None => {
                let err = lib.error();
                if err == 0 {
                    return EXIT_TIMEOUT;
                } else if err != libc::EINTR {
                    ceprint!(strerror(err), "\n");
                    return EXIT_FAILURE;
                }
            }
            // TODO: replace filename of renamed filesystem watch entries
            Some(_) if scope != WatchScope::Inode => {}
            Some(event) => moves.handle(&mut lib, &event, recursive, events, false, &mut out),
        }

        if DONE.load(Ordering::SeqCst) {
            break;
        }
    }

    print_info(&mut lib, &o)
}

/// Print statistics requested by SIGUSR1.
fn check_print_now(lib: &mut Inotifytools, o: &Opts) {
    if PRINT_NOW.swap(false, Ordering::SeqCst) {
        print_info(lib, o);
        cprint!("\n");
    }
}

/// The table columns: (event, header, width of the value column).
const COLUMNS: [(i32, &str, usize); 13] = [
    (IN_ACCESS, "access", 6),
    (IN_MODIFY, "modify", 6),
    (IN_ATTRIB, "attrib", 6),
    (IN_CLOSE_WRITE, "close_write", 11),
    (IN_CLOSE_NOWRITE, "close_nowrite", 13),
    (IN_OPEN, "open", 4),
    (IN_MOVED_FROM, "moved_from", 10),
    (IN_MOVED_TO, "moved_to", 8),
    (IN_MOVE_SELF, "move_self", 9),
    (IN_CREATE, "create", 6),
    (IN_DELETE, "delete", 6),
    (IN_DELETE_SELF, "delete_self", 11),
    (IN_UNMOUNT, "unmount", 7),
];

fn print_info(lib: &mut Inotifytools, o: &Opts) -> i32 {
    if lib.get_stat_total(0) == 0 {
        ceprint!("No events occurred.\n");
        return EXIT_SUCCESS;
    }

    let shown: Vec<(i32, &str, usize)> = COLUMNS
        .iter()
        .copied()
        .filter(|&(ev, _, _)| (ev & o.events) != 0 && (o.zero != 0 || lib.get_stat_total(ev) != 0))
        .collect();

    // OK, go through the watches and print stats.
    cprint!("total  ");
    for (_, name, _) in &shown {
        cprint!(name, "  ");
    }
    cprint!("filename\n");

    for (w, stats) in lib.watches_sorted_by_event(o.sort) {
        if o.zero == 0 && stats.total == 0 {
            continue;
        }
        cprint!(format!("{:<5}  ", stats.total));
        for &(ev, _, width) in &shown {
            let v = stat(&stats, ev);
            cprint!(format!("{:<width$}  ", v, width = width));
        }
        cprint!(lib.filename_from_watch(w), "\n");
    }

    EXIT_SUCCESS
}

fn stat(s: &WatchStats, ev: i32) -> u32 {
    s.get(ev).unwrap_or(0)
}

const OPT_STRING: &str = "hrPa:d:zve:t:IFMS";

const LONG_OPTS: &[LongOpt] = &[
    ("help", false, b'h'),
    ("event", true, b'e'),
    ("timeout", true, b't'),
    ("verbose", false, b'v'),
    ("zero", false, b'z'),
    ("ascending", true, b'a'),
    ("descending", true, b'd'),
    ("recursive", false, b'r'),
    ("inotify", false, b'I'),
    ("fanotify", false, b'F'),
    ("filesystem", false, b'S'),
    ("mount", false, b'M'),
    ("no-dereference", false, b'P'),
    ("fromfile", true, b'o'),
    ("exclude", true, b'c'),
    ("excludei", true, b'b'),
    ("include", true, b'j'),
    ("includei", true, b'k'),
];

fn sort_key(optarg: &[u8]) -> Option<i32> {
    let event = inotifytools::str_to_event(Some(optarg));
    if event == -1 {
        ceprint!("'", optarg, "' is not a valid key for sorting!\n");
        return None;
    }
    Some(event)
}

/// Parse the command line.  Returns the remaining (file) arguments, or
/// `None` if the program should exit with failure.
fn parse_opts(g: &mut GetOpt, o: &mut Opts) -> Option<Vec<Vec<u8>>> {
    let mut sort_set = false;
    let arg = |g: &GetOpt| g.optarg().map(|a| a.to_bytes().to_vec());

    let mut curr_opt = g.next();
    while curr_opt != OPT_ERR && curr_opt != OPT_END {
        match curr_opt as u8 {
            b'h' => {
                print_help(&help_name(g));
                return None;
            }
            b'v' => o.verbose += 1,
            b'r' => o.recursive += 1,
            b'I' | b'S' | b'M' => {
                if !set_watch_scope(&mut o.scope, &mut o.fanotify, curr_opt as u8) {
                    return None;
                }
            }
            b'F' => o.fanotify = true,
            b'P' => o.no_dereference += 1,
            b'z' => o.zero += 1,
            b'c' => o.filters.exc = arg(g),
            b'b' => o.filters.exci = arg(g),
            b'j' => o.filters.inc = arg(g),
            b'k' => o.filters.inci = arg(g),
            b'o' => {
                if o.fromfile.is_some() {
                    ceprint!("Multiple --fromfile options given.\n");
                    return None;
                }
                o.fromfile = arg(g);
            }
            b't' => {
                if !is_timeout_option_valid(&mut o.timeout, g.optarg()) {
                    return None;
                }
            }
            b'e' => o.events |= parse_event(g.optarg())?,
            b'a' => {
                let optarg = arg(g).unwrap_or_default();
                if sort_set {
                    ceprint!("Please specify -a or -d once only!\n");
                    return None;
                }

                if optarg.eq_ignore_ascii_case(b"total") {
                    o.sort = 0;
                } else if optarg.eq_ignore_ascii_case(b"move") {
                    ceprint!(
                        "Cannot sort by `move' event; please use `moved_from' or `moved_to'.\n"
                    );
                    return None;
                } else if optarg.eq_ignore_ascii_case(b"close") {
                    ceprint!(
                        "Cannot sort by `close' event; please use `close_write' or `close_nowrite'.\n"
                    );
                    return None;
                } else {
                    o.sort = sort_key(&optarg)?;
                }
                sort_set = true;
            }
            b'd' => {
                // Note: like the original, -d does not mark the sort key as
                // set, so it may be given several times (or followed by -a).
                let optarg = arg(g).unwrap_or_default();
                if sort_set {
                    ceprint!("Please specify -a or -d once only!\n");
                    return None;
                }

                if optarg.eq_ignore_ascii_case(b"total") {
                    o.sort = -1;
                } else {
                    o.sort = sort_key(&optarg)?.wrapping_neg();
                }
            }
            _ => {}
        }

        curr_opt = g.next();
    }

    let rest = g.remaining();

    let watched =
        if o.events != 0 { o.events } else { o.scope.unwrap_or_default().default_events() };
    if o.sort != 0 && o.sort != -1 && (o.sort.wrapping_abs() & watched) == 0 {
        ceprint!("Can't sort by an event which isn't being watched for!\n");
        return None;
    }

    if !o.filters.validate() {
        return None;
    }

    // If ? returned, invalid option
    if curr_opt == OPT_ERR {
        return None;
    }
    Some(rest)
}

fn print_help(tool_name: &[u8]) {
    let t = tool_name;
    cprint!(t, " ", package_version(), "\n");
    cprint!("Gather filesystem usage statistics using ", t, ".\n");
    cprint!("Usage: ", t, " [ options ] file1 [ file2 ] [ ... ]\n");
    cprint!("Options:\n");
    cprint!("\t-h|--help    \tShow this help text.\n");
    cprint!("\t-v|--verbose \tBe verbose.\n");
    cprint!("\t@<file>       \tExclude the specified file from being watched.\n");
    cprint!("\t--fromfile <file>\n", "\t\tRead files to watch from <file> or `-' for stdin.\n");
    cprint!(
        "\t--exclude <pattern>\n",
        "\t\tExclude all events on files matching the extended regular\n",
        "\t\texpression <pattern>.\n"
    );
    cprint!("\t--excludei <pattern>\n", "\t\tLike --exclude but case insensitive.\n");
    cprint!(
        "\t--include <pattern>\n",
        "\t\tExclude all events on files except the ones\n",
        "\t\tmatching the extended regular expression\n",
        "\t\t<pattern>.\n"
    );
    cprint!("\t--includei <pattern>\n", "\t\tLike --include but case insensitive.\n");
    cprint!(
        "\t-z|--zero\n",
        "\t\tIn the final table of results, output rows and columns even\n",
        "\t\tif they consist only of zeros (the default is to not output\n",
        "\t\tthese rows and columns).\n"
    );
    cprint!("\t-r|--recursive\tWatch directories recursively.\n");
    cprint!("\t-I|--inotify\tWatch with inotify.\n");
    cprint!("\t-F|--fanotify\tWatch with fanotify.\n");
    cprint!("\t-S|--filesystem\tWatch entire filesystem with fanotify.\n");
    cprint!("\t-M|--mount\tWatch entire mount with fanotify.\n");
    cprint!("\t-P|--no-dereference\n", "\t\tDo not follow symlinks.\n");
    cprint!(
        "\t-t|--timeout <seconds>\n",
        "\t\tListen only for specified amount of time in seconds; if\n",
        "\t\tomitted or zero, ",
        t,
        " will execute until receiving an\n",
        "\t\tinterrupt signal.\n"
    );
    cprint!(
        "\t-e|--event <event1> [ -e|--event <event2> ... ]\n",
        "\t\tListen for specific event(s).  If omitted, all events are \n",
        "\t\tlistened for.\n"
    );
    cprint!(
        "\t-a|--ascending <event>\n",
        "\t\tSort ascending by a particular event, or `total'.\n"
    );
    cprint!(
        "\t-d|--descending <event>\n",
        "\t\tSort descending by a particular event, or `total'.\n\n"
    );
    cprint!("Exit status:\n");
    cprint!("\t", EXIT_SUCCESS, "  -  Exited normally.\n");
    cprint!("\t", EXIT_FAILURE, "  -  Some error occurred.\n\n");
    cprint!("Events:\n");
    print_event_descriptions();
}
