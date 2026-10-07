//! Code shared by inotifywait and inotifywatch.

#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;

use inotifytools::cio::{errno, set_errno, strerror};
use inotifytools::consts::*;
use inotifytools::sys;
use inotifytools::{cat, ceprint, cprint, Event, Inotifytools, WatchScope};
use libc::{c_char, c_int, c_long};

pub const BLOCKING_TIMEOUT: c_long = 0;

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_FAILURE: i32 = 1;
pub const EXIT_TIMEOUT: i32 = 2;

const MAXLEN: usize = 4096;

/// The package version, e.g. "4.26.261" (see packaging/version.sh).
pub fn package_version() -> &'static str {
    env!("INOTIFY_TOOLS_VERSION")
}

pub fn print_event_descriptions() {
    cprint!(
        "\taccess\t\tfile or directory contents were read\n",
        "\tmodify\t\tfile or directory contents were written\n",
        "\tattrib\t\tfile or directory attributes changed\n",
        "\tclose_write\tfile or directory closed, after being opened in\n",
        "\t           \twritable mode\n",
        "\tclose_nowrite\tfile or directory closed, after being opened in\n",
        "\t           \tread-only mode\n",
        "\tclose\t\tfile or directory closed, regardless of read/write mode\n",
        "\topen\t\tfile or directory opened\n",
        "\tmoved_to\tfile or directory moved to watched directory\n",
        "\tmoved_from\tfile or directory moved from watched directory\n",
        "\tmove\t\tfile or directory moved to or from watched directory\n",
        "\tmove_self\t\tA watched file or directory was moved.\n",
        "\tcreate\t\tfile or directory created within watched directory\n",
        "\tdelete\t\tfile or directory deleted within watched directory\n",
        "\tdelete_self\tfile or directory was deleted\n",
        "\tunmount\t\tfile system containing file or directory unmounted\n",
    );
}

pub use inotifytools::isdir;

/// Files to watch and files to exclude (given as `@file`).
#[derive(Debug, Default)]
pub struct FileList {
    pub watch_files: Vec<Vec<u8>>,
    pub exclude_files: Vec<Vec<u8>>,
}

impl FileList {
    fn add(&mut self, name: &[u8]) {
        if name.is_empty() || name == b"@" {
            return;
        }
        if name[0] == b'@' {
            self.exclude_files.push(name[1..].to_vec());
        } else {
            self.watch_files.push(name.to_vec());
        }
    }
}

/// Read one line like `fgets(buf, MAXLEN, f)`: at most MAXLEN-1 bytes,
/// stopping after a newline.  Returns `None` at end of file.
fn fgets(f: *mut libc::FILE) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; MAXLEN];
    let p = unsafe { libc::fgets(buf.as_mut_ptr().cast(), MAXLEN as c_int, f) };
    if p.is_null() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(MAXLEN);
    buf.truncate(len);
    Some(buf)
}

/// Build the list of files to watch from `--fromfile` (if given) followed
/// by the command line arguments.  Returns `None` if the file could not be
/// opened (after printing an error).
pub fn construct_path_list(args: &[Vec<u8>], fromfile: Option<&[u8]>) -> Option<FileList> {
    let mut list = FileList::default();

    let mut file: *mut libc::FILE = std::ptr::null_mut();
    let mut close = false;
    if let Some(filename) = fromfile {
        if filename == b"-" {
            file = inotifytools::cio::c_stdin();
        } else {
            let c = inotifytools::cio::cstring(filename);
            file = unsafe { libc::fopen(c.as_ptr(), b"r\0".as_ptr().cast()) };
            if file.is_null() {
                ceprint!("Couldn't open ", filename, ": ", strerror(errno()), "\n");
                return None;
            }
            close = true;
        }
    }

    if !file.is_null() {
        while let Some(mut name) = fgets(file) {
            // Note: an empty line is kept as an (empty) file name, like the
            // original implementation.
            if name.is_empty() {
                continue;
            }
            let str_len = name.len();
            if name[str_len - 1] == b'\n' {
                name.truncate(str_len - 1);
            }
            if str_len == 1 && name.first() == Some(&b'@') {
                continue;
            }
            if let Some(b'@') = name.first() {
                list.exclude_files.push(name[1..].to_vec());
                continue;
            }
            list.watch_files.push(name);
        }
        if close {
            unsafe { libc::fclose(file) };
        }
    }

    for a in args {
        list.add(a);
    }

    Some(list)
}

pub fn warn_inotify_init_error(lib: &Inotifytools, fanotify: bool) {
    let backend = if fanotify { "fanotify" } else { "inotify" };
    let resource = if fanotify { "groups" } else { "instances" };
    let error = lib.error();

    ceprint!("Couldn't initialize ", backend, ": ", strerror(error), "\n");
    if error == libc::EMFILE {
        ceprint!(
            "Try increasing the value of /proc/sys/fs/",
            backend,
            "/max_user_",
            resource,
            "\n"
        );
    }
    if fanotify && error == libc::EINVAL {
        ceprint!(
            "fanotify support for reporting the events with file names was added in kernel v5.9.\n"
        );
    }
    if fanotify && error == libc::EPERM {
        ceprint!("fanotify watch requires admin privileges\n");
    }
}

/// Parse a `--timeout` value with `strtol()` semantics.
pub fn is_timeout_option_valid(timeout: &mut c_long, o: Option<&CStr>) -> bool {
    let o = match o {
        Some(o) if !o.to_bytes().is_empty() => o,
        _ => {
            ceprint!(
                "The provided value is not a valid timeout value.\n",
                "Please specify a long int value.\n"
            );
            return false;
        }
    };

    let mut end: *mut c_char = std::ptr::null_mut();
    set_errno(0);
    *timeout = unsafe { libc::strtol(o.as_ptr(), &mut end, 10) };
    let e = errno();

    if e != 0 {
        ceprint!("Something went wrong with the timeout value you provided.\n");
        ceprint!(strerror(e), "\n");
        return false;
    }

    // strtol() always sets `end`; check that it consumed the whole string.
    let consumed = end as usize - o.as_ptr() as usize;
    if consumed != o.to_bytes().len() {
        ceprint!("'", o, "' is not a valid timeout value.\n", "Please specify a long int value.\n");
        return false;
    }

    true
}

/// POSIX `basename()` (as from `<libgen.h>`).
pub fn basename(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return b".".to_vec();
    }
    let trimmed = {
        let mut end = path.len();
        while end > 1 && path[end - 1] == b'/' {
            end -= 1;
        }
        &path[..end]
    };
    if trimmed == b"/" {
        return b"/".to_vec();
    }
    match trimmed.iter().rposition(|&c| c == b'/') {
        Some(i) => trimmed[i + 1..].to_vec(),
        None => trimmed.to_vec(),
    }
}

/// Whether the program was invoked as one of the fsnotify* names.
pub fn invoked_as_fsnotify(argv0: Option<&[u8]>) -> bool {
    argv0.map_or(false, |a| basename(a).starts_with(b"fsnotify"))
}

/// Command line parsing through the C library's `getopt_long()`, so that
/// option syntax (abbreviations, permutation, `--`) and error messages are
/// exactly those of the original programs.
pub struct GetOpt {
    _args: Vec<CString>,
    argv: Vec<*mut c_char>,
    optstring: CString,
    _names: Vec<CString>,
    longopts: Vec<sys::option>,
}

/// A long option: (name, takes an argument, value returned).
pub type LongOpt = (&'static str, bool, u8);

impl GetOpt {
    pub fn new(optstring: &str, longs: &[LongOpt]) -> GetOpt {
        let args: Vec<CString> =
            std::env::args_os().map(|a| inotifytools::cio::cstring(a.as_bytes())).collect();
        let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
        argv.push(std::ptr::null_mut());
        let names: Vec<CString> = longs.iter().map(|l| CString::new(l.0).unwrap()).collect();
        let mut longopts: Vec<sys::option> = longs
            .iter()
            .zip(&names)
            .map(|(l, n)| sys::option {
                name: n.as_ptr(),
                has_arg: if l.1 { sys::REQUIRED_ARGUMENT } else { sys::NO_ARGUMENT },
                flag: std::ptr::null_mut(),
                val: l.2 as c_int,
            })
            .collect();
        longopts.push(sys::option {
            name: std::ptr::null(),
            has_arg: 0,
            flag: std::ptr::null_mut(),
            val: 0,
        });
        GetOpt {
            _args: args,
            argv,
            optstring: CString::new(optstring).unwrap(),
            _names: names,
            longopts,
        }
    }

    pub fn argc(&self) -> c_int {
        (self.argv.len() - 1) as c_int
    }

    pub fn argv0(&self) -> Option<Vec<u8>> {
        let p = self.argv[0];
        if p.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(p) }.to_bytes().to_vec())
        }
    }

    /// Next option character, `'?'` on error, -1 at the end (as `char`,
    /// like the original programs stored it).
    pub fn next(&mut self) -> c_int {
        let r = unsafe {
            sys::getopt_long(
                self.argc(),
                self.argv.as_ptr(),
                self.optstring.as_ptr(),
                self.longopts.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        r as c_char as c_int
    }

    /// The current option's argument.
    pub fn optarg(&self) -> Option<&'static CStr> {
        let p = unsafe { sys::optarg };
        if p.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(p) })
        }
    }

    /// The non-option arguments left after parsing.
    pub fn remaining(&self) -> Vec<Vec<u8>> {
        let optind = unsafe { sys::optind }.max(0) as usize;
        self.argv[..self.argv.len() - 1]
            .iter()
            .skip(optind)
            .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes().to_vec())
            .collect()
    }
}

/// The `--exclude`, `--excludei`, `--include` and `--includei` options.
#[derive(Default)]
pub struct Filters {
    pub exc: Option<Vec<u8>>,
    pub exci: Option<Vec<u8>>,
    pub inc: Option<Vec<u8>>,
    pub inci: Option<Vec<u8>>,
}

impl Filters {
    pub fn has_include(&self) -> bool {
        self.inc.is_some() || self.inci.is_some()
    }

    /// Reject conflicting options (with a message).
    pub fn validate(&self) -> bool {
        let msg = if self.exc.is_some() && self.exci.is_some() {
            "--exclude and --excludei cannot both be specified.\n"
        } else if self.inc.is_some() && self.inci.is_some() {
            "--include and --includei cannot both be specified.\n"
        } else if self.has_include() && (self.exc.is_some() || self.exci.is_some()) {
            "include and exclude regexp cannot both be specified.\n"
        } else {
            return true;
        };
        ceprint!(msg);
        false
    }

    /// Install the filters; false (with a message) on an invalid regex.
    pub fn install(&self, lib: &mut Inotifytools, recursive: c_int) -> bool {
        let icase = sys::REG_EXTENDED | sys::REG_ICASE;
        let exc = [(&self.exc, sys::REG_EXTENDED), (&self.exci, icase)];
        for (re, flags) in exc {
            if re.is_some() && !lib.ignore_events_by_regex(re.as_deref(), flags, recursive) {
                ceprint!("Error in `exclude' regular expression.\n");
                return false;
            }
        }
        let inc = [(&self.inc, sys::REG_EXTENDED), (&self.inci, icase)];
        for (re, flags) in inc {
            if re.is_some() && !lib.ignore_events_by_inverted_regex(re.as_deref(), flags, recursive)
            {
                ceprint!("Error in `include' regular expression.\n");
                return false;
            }
        }
        true
    }
}

/// Record `-I`, `-S`, or `-M` in `scope`. Returns false when one was already given.
pub fn set_watch_scope(scope: &mut Option<WatchScope>, fanotify: &mut bool, opt: u8) -> bool {
    if scope.is_some() {
        ceprint!("Please specify -I -S or -M once only!\n");
        return false;
    }
    match opt {
        b'I' => {
            *scope = Some(WatchScope::Inode);
            *fanotify = false;
        }
        b'M' => {
            *scope = Some(WatchScope::Mount);
            *fanotify = true;
        }
        _ => {
            *scope = Some(WatchScope::Filesystem);
            *fanotify = true;
        }
    }
    true
}

/// Parse an `--event` argument (printing an error if invalid).
pub fn parse_event(optarg: Option<&CStr>) -> Option<i32> {
    let optarg = optarg.map(|a| a.to_bytes()).unwrap_or_default();
    let event = inotifytools::str_to_event(Some(optarg));
    if event == -1 {
        ceprint!(
            "'",
            optarg,
            "' is not a valid event!  Run with the '--help' option to see a list of events.\n"
        );
        return None;
    }
    Some(event)
}

/// The tool name shown by `--help`.
pub fn help_name(g: &GetOpt) -> Vec<u8> {
    match g.argv0() {
        Some(a) if g.argc() > 0 => basename(&a),
        _ => b"<executable>".to_vec(),
    }
}

/// Set up the watches for `list`.  `out` receives error messages, `before`
/// and `after` are called around each inode watch.
#[allow(clippy::too_many_arguments)]
pub fn watch_list(
    lib: &mut Inotifytools,
    list: &FileList,
    events: i32,
    recursive: bool,
    scope: WatchScope,
    fanotify: bool,
    fail_verb: &str,
    out: &mut dyn FnMut(Vec<u8>),
    before: &mut dyn FnMut(&[u8]),
    after: &mut dyn FnMut(&[u8]),
) -> bool {
    let backend = if fanotify { "fanotify" } else { "inotify" };
    let resource = if fanotify { "marks" } else { "watches" };
    for this_file in &list.watch_files {
        before(this_file);
        if scope != WatchScope::Inode {
            let all: Vec<&[u8]> = list.watch_files.iter().map(|v| v.as_slice()).collect();
            if !lib.watch_files(&all, events) {
                let e = strerror(lib.error());
                out(cat!("Couldn't add ", scope.name(), " watch ", this_file, ": ", e, "\n"));
                return false;
            }
            return true;
        }
        let ok = if recursive {
            lib.watch_recursively_with_exclude(this_file, events, &list.exclude_files)
        } else {
            lib.watch_file(this_file, events)
        };
        if !ok {
            if lib.error() == libc::ENOSPC {
                out(cat!(
                    "Failed to watch ",
                    this_file,
                    "; upper limit on ",
                    backend,
                    " ",
                    resource,
                    " reached!\n"
                ));
                out(cat!(
                    "Please increase the amount of ",
                    backend,
                    " ",
                    resource,
                    " allowed per user via `/proc/sys/fs/",
                    backend,
                    "/max_user_",
                    resource,
                    "'.\n"
                ));
            } else {
                out(cat!(fail_verb, this_file, ": ", strerror(lib.error()), "\n"));
            }
            return false;
        }
        after(this_file);
    }
    true
}

/// Follows directory creation and moves to keep recursive watches current.
#[derive(Default)]
pub struct MoveTracker {
    moved_from: Option<Vec<u8>>,
}

impl MoveTracker {
    /// Handle an event.  With `announce`, new directories are reported
    /// through `out`, which also receives errors.
    pub fn handle(
        &mut self,
        lib: &mut Inotifytools,
        event: &Event,
        recursive: bool,
        events: i32,
        announce: bool,
        out: &mut dyn FnMut(Vec<u8>),
    ) {
        let mask = event.mask as i32;
        // MOVED_FROM not followed by MOVED_TO: it left the tree, unwatch it.
        if let Some(mf) = &self.moved_from {
            if mask & IN_MOVED_TO == 0 {
                if !lib.remove_watch_by_filename(mf) {
                    out(cat!("Error removing watch on ", mf, ": ", strerror(lib.error()), "\n"));
                }
                self.moved_from = None;
            }
        }
        if !recursive {
            return;
        }

        if mask & IN_CREATE != 0 || (self.moved_from.is_none() && mask & IN_MOVED_TO != 0) {
            // New file - if it is a directory, watch it
            let new_file = match lib.dirpath_from_event(event) {
                Some(f) if !f.is_empty() && isdir(&f) => f,
                _ => return,
            };
            if announce {
                out(cat!("Watching new directory ", new_file, "\n"));
            }
            if !lib.watch_recursively(&new_file, events) {
                let e = strerror(lib.error());
                out(cat!("Couldn't watch new directory ", new_file, ": ", e, "\n"));
            }
        } else if mask & IN_MOVED_FROM != 0 {
            self.moved_from = lib.dirpath_from_event(event);
            // if not watched...
            if lib.wd_from_filename(self.moved_from.as_deref().unwrap_or_default()) == -1 {
                self.moved_from = None;
            }
        } else if mask & IN_MOVED_TO != 0 {
            if let Some(mf) = self.moved_from.take() {
                let new_name = lib.dirpath_from_event(event).unwrap_or_default();
                lib.replace_filename(&mf, &new_name);
            }
        }
    }
}

/// `(char)-1`: getopt's end marker, compared the way the original did
/// (`char` is unsigned on some architectures).
pub const OPT_END: c_int = -1i32 as c_char as c_int;
pub const OPT_ERR: c_int = b'?' as c_int;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_basename() {
        assert_eq!(basename(b""), b".");
        assert_eq!(basename(b"/"), b"/");
        assert_eq!(basename(b"///"), b"/");
        assert_eq!(basename(b"/usr/bin/inotifywait"), b"inotifywait");
        assert_eq!(basename(b"../../src/fsnotifywait/"), b"fsnotifywait");
        assert_eq!(basename(b"inotifywatch"), b"inotifywatch");
        assert!(invoked_as_fsnotify(Some(b"/usr/bin/fsnotifywait")));
        assert!(!invoked_as_fsnotify(Some(b"/usr/bin/inotifywait")));
        assert!(!invoked_as_fsnotify(None));
    }

    #[test]
    fn csv_like_file_list() {
        let args = vec![b"a".to_vec(), b"@b".to_vec(), b"@".to_vec(), b"".to_vec(), b"c".to_vec()];
        let l = construct_path_list(&args, None).unwrap();
        assert_eq!(l.watch_files, vec![b"a".to_vec(), b"c".to_vec()]);
        assert_eq!(l.exclude_files, vec![b"b".to_vec()]);
    }
}
