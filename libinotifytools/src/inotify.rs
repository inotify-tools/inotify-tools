//! Port of the original `inotifytools.cpp` and `stats.cpp`.  Storage the C
//! API hands out pointers to is allocated once and reused in place, so those
//! pointers stay valid exactly as long as they did in C.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, CString};
use std::mem;
use std::ptr;

use libc::{
    c_char, c_int, c_long, c_uint, pid_t, EACCES, EINVAL, ELOOP, EMSGSIZE, ENOENT, ENOTDIR,
};

use crate::cio::{cstring, errno, strerror};
use crate::consts::*;
#[cfg(target_os = "linux")]
use crate::fid::{self, FidKey};
use crate::sys;

/// Maximum length of strings produced by the printf-like functions.
pub const MAX_STRLEN: usize = 4096;
pub(crate) const MAX_EVENTS: usize = 4096;
/// `sizeof(struct inotify_event)`
const EVENT_SIZE: usize = 16;
const PATH_MAX: usize = libc::PATH_MAX as usize;
/// Size of the buffer holding filenames resolved from fanotify fids.
const FIDNAME_SIZE: usize = PATH_MAX + 512;

const WATCHES_SIZE_PATH: &[u8] = b"/proc/sys/fs/inotify/max_user_watches\0";
// Sic: the original library reads this (non-existent) file, so
// inotifytools_get_max_queued_events() always fails.  Kept for compatibility.
const QUEUE_SIZE_PATH: &[u8] = b"/proc/sys/fs/inotify/max_queued_watches\0";
const INSTANCES_PATH: &[u8] = b"/proc/sys/fs/inotify/max_user_instances\0";

pub(crate) const EMPTY: *const c_char = b"\0".as_ptr() as *const c_char;

#[cfg(target_os = "linux")]
type FidMap = BTreeMap<FidKey, u64>;
#[cfg(not(target_os = "linux"))]
type FidMap = BTreeMap<Vec<u8>, u64>;

/// Print a diagnostic in the format of the C library's `niceassert()` when
/// `cond` is false.  Evaluates to `cond`.
macro_rules! niceassert {
    ($cond:expr, $condstr:expr, $msg:expr) => {{
        let c: bool = $cond;
        if !c {
            nice_fail(line!(), $condstr, $msg);
        }
        c
    }};
}

fn nice_fail(line: u32, condstr: &str, msg: &str) {
    let tail = if msg.is_empty() { ".".to_string() } else { format!(": {}", msg) };
    ceprint!("inotifytools.rs:", line, " assertion ( ", condstr, " ) failed", tail, "\n");
}

/// A string that may contain any byte including NUL; layout-compatible with
/// the C `struct nstring`.
#[repr(C)]
pub struct NString {
    pub buf: [u8; MAX_STRLEN],
    pub len: c_uint,
}

impl NString {
    pub fn new() -> Box<NString> {
        Box::new(NString { buf: [0; MAX_STRLEN], len: 0 })
    }

    /// The valid contents (`len` bytes, clamped to the buffer size).
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..(self.len as usize).min(MAX_STRLEN)]
    }
}

/// An inotify event (also used for fanotify events converted to inotify
/// form).  `name` holds the bytes of the name up to (excluding) the first
/// NUL; `len` is the raw `inotify_event.len` field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub wd: i32,
    pub mask: u32,
    pub cookie: u32,
    pub len: u32,
    pub name: Vec<u8>,
}

/// Per-watch (or total) event counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatchStats {
    pub access: u32,
    pub modify: u32,
    pub attrib: u32,
    pub close_write: u32,
    pub close_nowrite: u32,
    pub open: u32,
    pub moved_from: u32,
    pub moved_to: u32,
    pub create: u32,
    pub delete: u32,
    pub delete_self: u32,
    pub unmount: u32,
    pub move_self: u32,
    pub total: u32,
}

impl WatchStats {
    /// The counter for a single event, or `total` for 0.  `None` for
    /// anything else (the C `stat_ptr()` returning NULL).
    pub fn get(&self, event: i32) -> Option<u32> {
        Some(match event {
            IN_ACCESS => self.access,
            IN_MODIFY => self.modify,
            IN_ATTRIB => self.attrib,
            IN_CLOSE_WRITE => self.close_write,
            IN_CLOSE_NOWRITE => self.close_nowrite,
            IN_OPEN => self.open,
            IN_MOVED_FROM => self.moved_from,
            IN_MOVED_TO => self.moved_to,
            IN_CREATE => self.create,
            IN_DELETE => self.delete,
            IN_DELETE_SELF => self.delete_self,
            IN_UNMOUNT => self.unmount,
            IN_MOVE_SELF => self.move_self,
            0 => self.total,
            _ => return None,
        })
    }

    fn record(&mut self, mask: u32) {
        let m = mask as i32;
        let inc = |c: &mut u32, flag: i32| {
            if m & flag != 0 {
                *c = c.wrapping_add(1);
            }
        };
        inc(&mut self.access, IN_ACCESS);
        inc(&mut self.modify, IN_MODIFY);
        inc(&mut self.attrib, IN_ATTRIB);
        inc(&mut self.close_write, IN_CLOSE_WRITE);
        inc(&mut self.close_nowrite, IN_CLOSE_NOWRITE);
        inc(&mut self.open, IN_OPEN);
        inc(&mut self.moved_from, IN_MOVED_FROM);
        inc(&mut self.moved_to, IN_MOVED_TO);
        inc(&mut self.create, IN_CREATE);
        inc(&mut self.delete, IN_DELETE);
        inc(&mut self.delete_self, IN_DELETE_SELF);
        inc(&mut self.unmount, IN_UNMOUNT);
        inc(&mut self.move_self, IN_MOVE_SELF);
        self.total = self.total.wrapping_add(1);
    }
}

/// An opaque reference to a watch, see
/// [`Inotifytools::watches_sorted_by_event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchRef(u64);

/// Where the path of an event comes from; see
/// [`Inotifytools::filename_from_event`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventPath {
    /// The full filename recorded for the event's watch (for fanotify this
    /// may already include the event's file name).
    pub filename: Vec<u8>,
    /// Length of the directory part of `filename`.
    pub dirnamelen: usize,
    /// Name of the file within the directory (may be empty).
    pub eventname: Vec<u8>,
}

impl EventPath {
    /// The watched directory (or file) part: `filename[..dirnamelen]`.
    pub fn dirname(&self) -> &[u8] {
        &self.filename[..self.dirnamelen.min(self.filename.len())]
    }
}

pub(crate) struct Watch {
    fid: Option<Vec<u8>>,
    filename: CString,
    wd: i32,
    dirf: c_int,
    stats: WatchStats,
}

impl Drop for Watch {
    fn drop(&mut self) {
        if self.dirf != 0 {
            unsafe {
                libc::close(self.dirf);
            }
        }
    }
}

struct Regex(Box<sys::regex_t>);

impl Drop for Regex {
    fn drop(&mut self) {
        unsafe { sys::regfree(&mut *self.0) }
    }
}

/// Handle holding all libinotifytools state.
pub struct Inotifytools {
    fd: c_int,
    recursive_watch: c_int,
    collect_stats: bool,
    error: c_int,
    initialized: bool,
    verbosity: c_int,
    fanotify_mode: bool,
    fanotify_mark_type: u32,
    /// `AT_HANDLE_FID` for an inode watch when the kernel supports it, otherwise 0.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    at_handle_fid: c_int,
    self_pid: pid_t,

    watches: HashMap<u64, Box<Watch>>,
    next_id: u64,
    next_fanotify_wd: i32,
    by_wd: BTreeMap<i32, u64>,
    by_fid: FidMap,
    by_filename: BTreeMap<Vec<u8>, u64>,

    /// strftime format for `%T`; like the original, "" still counts as set.
    timefmt: Option<Vec<u8>>,
    regex: Option<Regex>,
    invert_regexp: bool,
    totals: WatchStats,

    // evbuf: first half raw kernel data, second half fanotify events
    // converted to inotify form.
    evbuf: Box<[u32]>,
    first_byte: i32,
    bytes: isize,
    this_bytes: isize,
    ret_off: usize,

    // Storage handed out by pointer through the C API.
    evstr: Box<[u8; 1024]>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fidname: Box<[u8; FIDNAME_SIZE]>,
    print_out: Option<Box<NString>>,
}

impl Drop for Inotifytools {
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl Default for Inotifytools {
    fn default() -> Self {
        Self::new()
    }
}

impl Inotifytools {
    /// Create an uninitialised handle; call [`Inotifytools::init`] next.
    pub fn new() -> Self {
        Inotifytools {
            fd: -1,
            recursive_watch: 0,
            collect_stats: false,
            error: 0,
            initialized: false,
            verbosity: 0,
            fanotify_mode: false,
            fanotify_mark_type: 0,
            at_handle_fid: 0,
            self_pid: 0,
            watches: HashMap::new(),
            next_id: 1,
            next_fanotify_wd: 0,
            by_wd: BTreeMap::new(),
            by_fid: BTreeMap::new(),
            by_filename: BTreeMap::new(),
            timefmt: None,
            regex: None,
            invert_regexp: false,
            totals: WatchStats::default(),
            evbuf: vec![0u32; 2 * MAX_EVENTS * EVENT_SIZE / 4].into_boxed_slice(),
            first_byte: 0,
            bytes: 0,
            this_bytes: 0,
            ret_off: 0,
            evstr: Box::new([0; 1024]),
            fidname: Box::new([0; FIDNAME_SIZE]),
            print_out: Some(NString::new()),
        }
    }

    // ----------------------------------------------------------------
    // Initialisation
    // ----------------------------------------------------------------

    /// Initialise inotify, or with `fanotify`, a fanotify group (watching
    /// whole filesystems with `watch_filesystem`).  Returns false on
    /// failure; see [`Inotifytools::error`].
    pub fn init(&mut self, fanotify: bool, watch_filesystem: bool, verbose: c_int) -> bool {
        if self.initialized {
            return true;
        }

        self.error = 0;
        self.verbosity = verbose;
        if fanotify {
            #[cfg(target_os = "linux")]
            unsafe {
                self.self_pid = libc::getpid();
                self.fanotify_mode = true;
                self.fanotify_mark_type =
                    if watch_filesystem { fid::FAN_MARK_FILESYSTEM } else { fid::FAN_MARK_INODE };
                // Inode watches only need identity, which overlayfs can encode
                // with AT_HANDLE_FID. Assume the flag works until a call rejects it.
                self.at_handle_fid = if self.fanotify_mark_type == fid::FAN_MARK_INODE {
                    fid::AT_HANDLE_FID
                } else {
                    0
                };
                self.fd = libc::fanotify_init(fid::FAN_REPORT_FID | fid::FAN_REPORT_DFID_NAME, 0);
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = watch_filesystem;
                self.fd = -1;
                crate::cio::set_errno(EINVAL);
            }
        } else {
            self.fanotify_mode = false;
            self.fd = unsafe { sys::inotify_init() };
        }
        if self.fd < 0 {
            self.error = errno();
            return false;
        }

        self.collect_stats = false;
        self.initialized = true;
        self.by_wd.clear();
        self.by_fid.clear();
        self.by_filename.clear();
        self.timefmt = None;
        true
    }

    /// Same as `init(false, false, 0)`.
    pub fn initialize(&mut self) -> bool {
        self.init(false, false, 0)
    }

    /// Close inotify and free all watches.  [`Inotifytools::init`] must be
    /// called again before further use.
    pub fn cleanup(&mut self) {
        if !self.initialized {
            return;
        }

        self.initialized = false;
        unsafe {
            libc::close(self.fd);
        }
        self.collect_stats = false;
        self.error = 0;
        self.timefmt = None;
        self.regex = None;

        self.by_wd.clear();
        self.by_fid.clear();
        self.by_filename.clear();
        self.watches.clear();
    }

    /// The last error which occurred (an `errno` value).
    pub fn error(&self) -> c_int {
        self.error
    }

    /// Whether the handle is in fanotify mode.
    pub fn fanotify_mode(&self) -> bool {
        self.fanotify_mode
    }

    // ----------------------------------------------------------------
    // Watch bookkeeping
    // ----------------------------------------------------------------

    fn watch_from_wd(&self, wd: i32) -> Option<u64> {
        self.by_wd.get(&wd).copied()
    }

    #[cfg(target_os = "linux")]
    fn watch_from_fid(&self, f: &[u8]) -> Option<u64> {
        self.by_fid.get(&FidKey::new(f)).copied()
    }

    fn watch_from_filename(&self, filename: &[u8]) -> Option<u64> {
        self.by_filename.get(filename).copied()
    }

    fn assert_init(&self) {
        niceassert!(self.initialized, "initialized", "inotifytools_initialize not called yet");
    }

    fn watch(&self, id: u64) -> &Watch {
        &self.watches[&id]
    }

    /// Register a new watch.  Like the original red-black tree insertion, an
    /// existing entry with the same key in any index is kept (not replaced).
    /// Returns the watch descriptor assigned.
    fn create_watch(
        &mut self,
        wd: i32,
        fid: Option<Vec<u8>>,
        filename: &[u8],
        dirf: c_int,
    ) -> Option<i32> {
        if wd < 0 {
            return None;
        }
        // fanotify has no watch descriptors; hand out unique positive ones.
        let wd = if wd != 0 {
            wd
        } else {
            loop {
                self.next_fanotify_wd = self.next_fanotify_wd.wrapping_add(1);
                if self.next_fanotify_wd <= 0 {
                    self.next_fanotify_wd = 1;
                }
                if !self.by_wd.contains_key(&self.next_fanotify_wd) {
                    break self.next_fanotify_wd;
                }
            }
        };

        let id = self.next_id;
        self.next_id += 1;
        let w = Box::new(Watch {
            fid,
            filename: cstring(filename),
            wd,
            dirf,
            stats: WatchStats::default(),
        });

        let mut inserted = false;
        if let Entry::Vacant(e) = self.by_wd.entry(wd) {
            e.insert(id);
            inserted = true;
        }
        #[cfg(target_os = "linux")]
        if let Some(f) = &w.fid {
            if let Entry::Vacant(e) = self.by_fid.entry(FidKey::new(f)) {
                e.insert(id);
                inserted = true;
            }
        }
        if let Entry::Vacant(e) = self.by_filename.entry(w.filename.to_bytes().to_vec()) {
            e.insert(id);
            inserted = true;
        }
        if inserted {
            self.watches.insert(id, w);
        }
        Some(wd)
    }

    /// Remove a watch from all indexes (by key, like the original) and
    /// destroy it.
    fn destroy_watch(&mut self, id: u64) {
        let w = self.watch(id);
        let (wd, name) = (w.wd, w.filename.to_bytes().to_vec());
        self.by_wd.remove(&wd);
        #[cfg(target_os = "linux")]
        if let Some(f) = &self.watch(id).fid {
            let key = FidKey::new(f);
            self.by_fid.remove(&key);
        }
        self.by_filename.remove(&name);
        self.watches.remove(&id);
    }

    fn remove_inotify_watch(&mut self, id: u64) -> bool {
        self.error = 0;
        let w = self.watch(id);
        // There is no kernel object representing the watch with fanotify.
        if w.fid.is_some() {
            return false;
        }
        let status = unsafe { sys::inotify_rm_watch(self.fd, w.wd) };
        if status < 0 {
            ceprint!("Failed to remove watch on ", w.filename, ": ", strerror(status), "\n");
            self.error = status;
            return false;
        }
        true
    }

    fn rename_watch(&mut self, id: u64, newname: &[u8]) {
        let old = self.watch(id).filename.to_bytes().to_vec();
        if self.by_filename.get(&old) == Some(&id) {
            self.by_filename.remove(&old);
        }
        let newname = cstring(newname);
        self.by_filename.entry(newname.to_bytes().to_vec()).or_insert(id);
        self.watches.get_mut(&id).unwrap().filename = newname;
    }

    /// Filename recorded for a watch (resolving fanotify filesystem watch
    /// fids).  The pointer stays valid until the watch is removed/renamed
    /// or, for resolved fids, until the next resolution.
    fn filename_from_watch_id(&mut self, id: u64) -> *const c_char {
        let w = self.watch(id);
        if w.fid.is_none() || self.fanotify_mark_type == 0 {
            return w.filename.as_ptr();
        }
        #[cfg(target_os = "linux")]
        {
            let f = w.fid.clone().unwrap();
            // The filename of a directory watch always ends with '/'. Do not
            // use the last event, it may describe another watch.
            let is_dir = w.filename.as_bytes().ends_with(b"/");
            if let Some(p) = self.filename_from_fid(&f, is_dir) {
                if unsafe { *p } != 0 {
                    return p;
                }
                // An empty path in the event means ESTALE - directory already gone.
                // Use the cached path but append the kernel's "(deleted)" suffix.
                // No space before the suffix after a directory's trailing '/'
                let cached = self.watch(id).filename.to_bytes().to_vec();
                let skip = (cached.last() == Some(&b'/')) as usize;
                let suffix = &b" (deleted)\0"[skip..];
                let buf = &mut self.fidname;
                let n = cached.len().min(buf.len().saturating_sub(suffix.len()));
                buf[..n].copy_from_slice(&cached[..n]);
                buf[n..n + suffix.len()].copy_from_slice(suffix);
                return buf.as_ptr().cast();
            }
        }
        self.watch(id).filename.as_ptr()
    }

    /// `inotifytools_filename_from_watch()` for an arbitrary (possibly
    /// bogus) watch pointer.
    pub(crate) fn filename_from_watch_ptr(&mut self, p: *const Watch) -> *const c_char {
        if p.is_null() {
            return EMPTY;
        }
        let id = self.watches.iter().find(|(_, w)| ptr::eq(&***w, p)).map(|(id, _)| *id);
        match id {
            Some(id) => self.filename_from_watch_id(id),
            None => EMPTY,
        }
    }

    pub(crate) fn filename_from_wd_ptr(&mut self, wd: i32) -> *const c_char {
        self.assert_init();
        if wd == 0 {
            return EMPTY;
        }
        match self.watch_from_wd(wd) {
            Some(id) => self.filename_from_watch_id(id),
            None => EMPTY,
        }
    }

    /// Get the filename used to establish a watch (empty if `wd` is not a
    /// watch descriptor).
    pub fn filename_from_wd(&mut self, wd: i32) -> Vec<u8> {
        let p = self.filename_from_wd_ptr(wd);
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }

    /// The watch filename and length of its directory part for an event.
    pub(crate) fn dirname_from_event_ptr(&mut self, wd: i32) -> (*const c_char, usize) {
        let filename = self.filename_from_wd_ptr(wd);
        let s = unsafe { CStr::from_ptr(filename) }.to_bytes();
        // Split dirname from filename for fanotify event
        let dirsep = if self.fanotify_mode { s.iter().rposition(|&c| c == b'/') } else { None };
        match dirsep {
            None => (filename, s.len()),
            Some(i) => (filename, i + 1),
        }
    }

    /// Returns (filename, dirnamelen, offset of the event name within
    /// filename, if it comes from there rather than from the event).
    pub(crate) fn filename_from_event_ptr(
        &mut self,
        wd: i32,
    ) -> (*const c_char, usize, Option<usize>) {
        let (f, dl) = self.dirname_from_event_ptr(wd);
        let s = unsafe { CStr::from_ptr(f) }.to_bytes();
        // On fanotify watch, filename includes event->name
        let off = if s.len() > dl { Some(dl) } else { None };
        (f, dl, off)
    }

    /// Get the watched path and file name of an event.
    pub fn filename_from_event(&mut self, ev: &Event) -> EventPath {
        let (f, dirnamelen, off) = self.filename_from_event_ptr(ev.wd);
        let filename = unsafe { CStr::from_ptr(f) }.to_bytes().to_vec();
        let eventname = match off {
            Some(o) => filename[o..].to_vec(),
            None if ev.len > 0 => ev.name.clone(),
            None => Vec::new(),
        };
        EventPath { filename, dirnamelen, eventname }
    }

    /// Get the directory path of an event on a directory ("dir/name/"), or
    /// `None` for events not on a directory.
    pub fn dirpath_from_event(&mut self, ev: &Event) -> Option<Vec<u8>> {
        let filename = self.filename_from_wd(ev.wd);
        if filename.is_empty() || ev.mask as i32 & IN_ISDIR == 0 {
            return None;
        }
        // fanotify watch->filename includes the name, so no need to add the
        // event->name again.
        let name: &[u8] = if self.fanotify_mode { b"" } else { &ev.name };
        Some(cat!(filename, name, "/"))
    }

    /// Get the watch descriptor for a filename (the name used to establish
    /// the watch), or -1.
    pub fn wd_from_filename(&self, filename: &[u8]) -> c_int {
        self.assert_init();
        if filename.is_empty() {
            return -1;
        }
        match self.watch_from_filename(filename) {
            Some(id) => self.watch(id).wd,
            None => -1,
        }
    }

    /// Set the filename for a watch descriptor.
    pub fn set_filename_by_wd(&mut self, wd: i32, filename: &[u8]) {
        self.assert_init();
        if let Some(id) = self.watch_from_wd(wd) {
            self.rename_watch(id, filename);
        }
    }

    /// Set the filename for the watch with a particular existing filename.
    pub fn set_filename_by_filename(&mut self, oldname: &[u8], newname: &[u8]) {
        if let Some(id) = self.watch_from_filename(oldname) {
            self.rename_watch(id, newname);
        }
    }

    /// Replace a filename prefix on all watches (e.g. after a directory was
    /// moved).
    pub fn replace_filename(&mut self, oldname: &[u8], newname: &[u8]) {
        if oldname.is_empty() || newname.is_empty() {
            return;
        }
        let entries: Vec<u64> = self.by_filename.values().copied().collect();
        for id in entries {
            let w = match self.watches.get(&id) {
                Some(v) => v,
                None => {
                    continue;
                }
            };
            let cur = w.filename.to_bytes();
            if cur.starts_with(oldname) && cur != newname {
                let name = cat!(newname, &cur[oldname.len()..]);
                self.rename_watch(id, &name);
            }
        }
    }

    fn remove_watch(&mut self, id: Option<u64>) -> bool {
        self.assert_init();
        let id = match id {
            Some(id) => id,
            None => return true,
        };
        if !self.remove_inotify_watch(id) {
            return false;
        }
        self.destroy_watch(id);
        true
    }

    /// Remove a watch by watch descriptor.  Returns true on success or if
    /// the watch does not exist.
    pub fn remove_watch_by_wd(&mut self, wd: i32) -> bool {
        self.remove_watch(self.watch_from_wd(wd))
    }

    /// Remove a watch by the filename used to establish it.
    pub fn remove_watch_by_filename(&mut self, filename: &[u8]) -> bool {
        self.remove_watch(self.watch_from_filename(filename))
    }

    /// Number of watches set up.
    pub fn get_num_watches(&self) -> c_int {
        self.by_filename.len() as c_int
    }

    // ----------------------------------------------------------------
    // Setting up watches
    // ----------------------------------------------------------------

    /// Set up a watch on a file.
    pub fn watch_file(&mut self, filename: &[u8], events: i32) -> bool {
        self.watch_files(&[filename], events)
    }

    /// Set up watches on a list of files.
    pub fn watch_files(&mut self, filenames: &[&[u8]], events: i32) -> bool {
        self.assert_init();
        self.error = 0;
        #[allow(unused_mut)]
        let mut events = events;

        for &fname in filenames {
            let cpath = cstring(fname);
            let fname = cpath.to_bytes();
            let wd: c_int;
            if self.fanotify_mode {
                #[cfg(target_os = "linux")]
                {
                    let mut flags = fid::FAN_MARK_ADD | self.fanotify_mark_type;
                    // Note: like the original, IN_DONT_FOLLOW is only honoured
                    // for the first file of the list.
                    if events & IN_DONT_FOLLOW != 0 {
                        events &= !IN_DONT_FOLLOW;
                        flags |= fid::FAN_MARK_DONT_FOLLOW;
                    }
                    let mask = (events | fid::FAN_EVENT_ON_CHILD) as i64 as u64;
                    wd = unsafe {
                        libc::fanotify_mark(self.fd, flags, mask, libc::AT_FDCWD, cpath.as_ptr())
                    };
                }
                #[cfg(not(target_os = "linux"))]
                {
                    wd = -1;
                }
            } else {
                wd = unsafe { sys::inotify_add_watch(self.fd, cpath.as_ptr(), events as u32) };
            }
            if wd < 0 {
                if wd == -1 {
                    self.error = errno();
                } else {
                    ceprint!(
                        "Failed to watch ",
                        fname,
                        ": returned wd was ",
                        wd,
                        " (expected -1 or >0 )"
                    );
                }
                return false;
            }

            // Always end filename with / if it is a directory
            let (filename, dirname): (Vec<u8>, Option<Vec<u8>>) = if !isdir(fname) {
                (fname.to_vec(), None)
            } else if fname.last() == Some(&b'/') {
                (fname.to_vec(), Some(fname.to_vec()))
            } else {
                let d = cat!(fname, "/");
                (d.clone(), Some(d))
            };

            #[allow(unused_mut)]
            let mut fidv: Option<Vec<u8>> = None;
            #[allow(unused_mut)]
            let mut dirf: c_int = 0;
            #[cfg(target_os = "linux")]
            if wd == 0 {
                match self.fanotify_encode_fid(&cpath, dirname.as_deref()) {
                    Some((f, d)) => {
                        fidv = Some(f);
                        dirf = d;
                    }
                    None => return false,
                }
            }
            let _ = &dirname;
            self.create_watch(wd, fidv, &filename, dirf);
        }

        true
    }

    /// Encode `path` with `name_to_handle_at`, setting `self.error` on
    /// failure. On return `handle[0]` is the handle length, or the required
    /// size when the buffer was too small.
    #[cfg(target_os = "linux")]
    fn name_to_handle(&mut self, path: &CStr, handle: &mut [u32], mount_id: &mut c_int) -> c_int {
        let flags = self.at_handle_fid;
        let mut encode_fid = |flags| {
            handle[0] = fid::MAX_FID_LEN as u32;
            unsafe {
                sys::name_to_handle_at(
                    libc::AT_FDCWD,
                    path.as_ptr(),
                    handle.as_mut_ptr().cast(),
                    mount_id,
                    flags,
                )
            }
        };
        let mut ret = encode_fid(flags);
        // Kernels before v6.6 reject AT_HANDLE_FID with EINVAL. Drop the
        // flag for the rest of this session and encode a regular handle.
        if ret != 0 && flags != 0 && errno() == EINVAL {
            self.at_handle_fid = 0;
            ret = encode_fid(0);
        }
        if ret != 0 {
            self.error = errno();
        }
        ret
    }

    /// Build the fid identifying a newly marked file (and register the
    /// filesystem's mount fd).  Returns (fid, directory O_PATH fd).
    #[cfg(target_os = "linux")]
    fn fanotify_encode_fid(
        &mut self,
        cpath: &CStr,
        dirname: Option<&[u8]>,
    ) -> Option<(Vec<u8>, c_int)> {
        let fname = cpath.to_bytes();
        let mut f = vec![0u8; fid::FID_HDR];

        let mut buf: libc::statfs = unsafe { mem::zeroed() };
        if unsafe { libc::statfs(cpath.as_ptr(), &mut buf) } != 0 {
            ceprint!("Statfs failed on ", fname, ": ", strerror(errno()), "\n");
            return None;
        }
        let fsid: [u8; 8] = unsafe { mem::transmute_copy(&buf.f_fsid) };
        f[4..12].copy_from_slice(&fsid);
        // btrfs fsid.val[1] differs per subvolume. Hash the mount fd by val[0].
        if buf.f_type as u32 as i64 == fid::BTRFS_SUPER_MAGIC {
            fid::clear_fsid_val1(&mut f);
        }

        // Hash mount_fd with fid->fsid (and null fhandle).  Note: the lookup
        // key still has hdr.len == 0 at this point, so (as in the original)
        // it never matches and every directory registers a mount fd.
        let mnt = if dirname.is_some() { self.watch_from_fid(&f) } else { None };
        if let (Some(d), None) = (dirname, mnt) {
            let fsidk = fid::fsid_key(&f);
            let cd = cstring(d);
            let mntid = unsafe { libc::open(cd.as_ptr(), libc::O_RDONLY) };
            if mntid < 0 {
                ceprint!("Failed to open ", d, ": ", strerror(errno()), "\n");
                return None;
            }
            // Hash mount_fd without terminating /
            self.create_watch(0, Some(fsidk), &d[..d.len() - 1], mntid);
        }

        let mut handle = [0u32; (8 + fid::MAX_FID_LEN) / 4];
        let mut mount_id: c_int = 0;
        let ret = self.name_to_handle(cpath, &mut handle, &mut mount_id);
        let hb = handle[0] as usize;
        if ret != 0 || hb > fid::MAX_FID_LEN {
            ceprint!("Encode fid failed on ", fname, ": ", strerror(errno()), "\n");
            return None;
        }
        f.resize(fid::FID_HDR + hb, 0);
        let hbytes: &[u8] =
            unsafe { std::slice::from_raw_parts(handle.as_ptr().cast::<u8>(), 8 + hb) };
        f[fid::HANDLE_OFF..fid::HANDLE_OFF + 8 + hb].copy_from_slice(hbytes);
        fid::set_info_type(
            &mut f,
            if dirname.is_some() {
                fid::FAN_EVENT_INFO_TYPE_DFID
            } else {
                fid::FAN_EVENT_INFO_TYPE_FID
            },
        );
        fid::set_hdr_len(&mut f, (fid::FID_HDR + hb) as u16);

        let mut dirf = 0;
        if let Some(d) = dirname {
            let cd = cstring(d);
            dirf = unsafe { libc::open(cd.as_ptr(), libc::O_PATH) };
            if dirf < 0 {
                ceprint!("Failed to open ", d, ": ", strerror(errno()), "\n");
                return None;
            }
        }
        Some((f, dirf))
    }

    /// Resolve the path of a fid (+ name) into the internal filename
    /// buffer.  Returns a pointer to it, or `None` to fall back to the
    /// stored filename.
    #[cfg(target_os = "linux")]
    fn filename_from_fid(&mut self, f: &[u8], is_dir: bool) -> Option<*const c_char> {
        let mut mount_fd = libc::AT_FDCWD;

        // Match mount_fd from fid->fsid (and null fhandle)
        if let Some(id) = self.watch_from_fid(&fid::fsid_key(f)) {
            mount_fd = self.watch(id).dirf;
        }

        let hb = fid::handle_bytes(f) as usize;
        let name_off = fid::FID_HDR + hb;
        let mut name_len: i32 = 0;
        if fid::info_type(f) == fid::FAN_EVENT_INFO_TYPE_DFID_NAME {
            let fid_len = (fid::FID_HDR + hb) as i32;
            name_len = fid::hdr_len(f) as i32 - fid_len;
            if name_len != 0 && f.get(name_off).copied().unwrap_or(0) == 0 {
                name_len = 0; // empty name??
            }
        }

        // Try to get path from file handle. fid is expected to be a directory
        // except for *_SELF events of non-directories.
        let self_nondir_fid = fid::info_type(f) == fid::FAN_EVENT_INFO_TYPE_FID && !is_dir;
        let flags = if self_nondir_fid { libc::O_PATH } else { libc::O_DIRECTORY };
        let mut h = fid::aligned_handle(f);
        let mut dirf = unsafe { sys::open_by_handle_at(mount_fd, h.as_mut_ptr().cast(), flags) };
        if dirf > 0 {
            // Got path by handle
        } else if self.fanotify_mark_type == fid::FAN_MARK_FILESYSTEM {
            // rm -rf delivers events for directories that are already gone.
            let e = errno();
            if e == libc::ESTALE {
                return Some(EMPTY);
            }
            ceprint!("Failed to decode directory fid (", strerror(e), ").\n");
            return None;
        } else if name_len != 0 {
            // For recursive watch look for watch by fid without the name
            let mut key = f.to_vec();
            if key.len() < fid::FID_HDR {
                key.resize(fid::FID_HDR, 0);
            }
            fid::set_info_type(&mut key, fid::FAN_EVENT_INFO_TYPE_DFID);
            fid::set_hdr_len(&mut key, (fid::hdr_len(f) as i32 - name_len) as u16);
            let id = match self.watch_from_fid(&key) {
                Some(v) => v,
                None => {
                    ceprint!("Failed to lookup path by directory fid.\n");
                    return None;
                }
            };
            let wdirf = self.watch(id).dirf;
            dirf = if wdirf != 0 { unsafe { libc::dup(wdirf) } } else { -1 };
            if dirf < 0 {
                ceprint!("Failed to get directory fd.\n");
                return None;
            }
        } else {
            // Fallthrough to stored filename
            return None;
        }

        let sym = CString::new(format!("/proc/self/fd/{}", dirf)).unwrap();
        let buf = &mut self.fidname;
        // PATH_MAX - 2 because we have to append two characters to this path,
        // '/' and 0
        let len = unsafe { libc::readlink(sym.as_ptr(), buf.as_mut_ptr().cast(), PATH_MAX - 2) };
        if len < 0 {
            ceprint!("Failed to resolve path from directory fd (", strerror(errno()), ").\n");
            unsafe { libc::close(dirf) };
            return None;
        }
        let mut len = len as usize;
        // Do not append '/' to a non-directory *_SELF path
        if !self_nondir_fid {
            buf[len] = b'/';
            len += 1;
        }
        buf[len] = 0;

        if name_len > 0 {
            let name: Vec<u8> =
                f.get(name_off..).unwrap_or(&[]).iter().take_while(|&&c| c != 0).copied().collect();
            let cname = cstring(&name);
            let deleted = unsafe {
                libc::faccessat(dirf, cname.as_ptr(), libc::F_OK, libc::AT_SYMLINK_NOFOLLOW)
            };
            let e = errno();
            if deleted != 0 && e != ENOENT {
                ceprint!("Failed to access file ", name, " (", strerror(e), ").\n");
                unsafe { libc::close(dirf) };
                return None;
            }
            let room = FIDNAME_SIZE - 16 - len;
            for i in 0..(name_len as usize).min(room) {
                buf[len + i] = f.get(name_off + i).copied().unwrap_or(0);
            }
            buf[(len + (name_len as usize).min(room)).min(FIDNAME_SIZE - 12)] = 0;
            if deleted != 0 {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(0);
                let suffix = b" (deleted)\0";
                if end + suffix.len() <= FIDNAME_SIZE {
                    buf[end..end + suffix.len()].copy_from_slice(suffix);
                }
            }
        }
        unsafe { libc::close(dirf) };
        Some(buf.as_ptr().cast())
    }

    /// Set up recursive watches on an entire directory tree.
    pub fn watch_recursively(&mut self, path: &[u8], events: i32) -> bool {
        self.watch_recursively_with_exclude(path, events, &[])
    }

    /// Set up recursive watches on a directory tree, excluding some
    /// directories (with or without trailing '/').
    pub fn watch_recursively_with_exclude(
        &mut self,
        path: &[u8],
        events: i32,
        exclude_list: &[Vec<u8>],
    ) -> bool {
        self.assert_init();

        self.error = 0;
        let cpath = cstring(path);
        let path = cpath.to_bytes();
        let dir = unsafe { libc::opendir(cpath.as_ptr()) };
        if dir.is_null() {
            let e = errno();
            // If not a directory, don't need to do anything special
            if e == ENOTDIR {
                return self.watch_file(path, events);
            }
            self.error = e;
            return false;
        }

        let my_path: Vec<u8> =
            if path.last() != Some(&b'/') { cat!(path, "/") } else { path.to_vec() };

        loop {
            let ent = unsafe { libc::readdir(dir) };
            if ent.is_null() {
                break;
            }
            let name = unsafe { CStr::from_ptr((*ent).d_name.as_ptr()) }.to_bytes().to_vec();
            if name != b"." && name != b".." {
                let next_file = cstring(&cat!(my_path, name));
                let mut st: libc::stat = unsafe { mem::zeroed() };
                if unsafe { libc::lstat(next_file.as_ptr(), &mut st) } == -1 {
                    let e = errno();
                    self.error = e;
                    if e != EACCES {
                        unsafe { libc::closedir(dir) };
                        return false;
                    }
                } else if st.st_mode & libc::S_IFMT == libc::S_IFDIR {
                    let next_file = cat!(my_path, name, "/");
                    if !is_excluded(&next_file, exclude_list) {
                        let status =
                            self.watch_recursively_with_exclude(&next_file, events, exclude_list);
                        // For some errors, we will continue.
                        if !status
                            && self.error != EACCES
                            && self.error != ENOENT
                            && self.error != ELOOP
                        {
                            unsafe { libc::closedir(dir) };
                            return false;
                        }
                    }
                }
            }
            self.error = 0;
        }

        unsafe { libc::closedir(dir) };
        self.watch_file(&my_path, events)
    }

    // ----------------------------------------------------------------
    // Reading events
    // ----------------------------------------------------------------

    fn buf(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.evbuf.as_ptr().cast::<u8>(), self.evbuf.len() * 4)
        }
    }

    fn buf_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.evbuf.as_mut_ptr().cast::<u8>(),
                self.evbuf.len() * 4,
            )
        }
    }

    pub(crate) fn event_ptr(&mut self) -> *mut sys::inotify_event {
        unsafe { self.evbuf.as_mut_ptr().cast::<u8>().add(self.ret_off).cast() }
    }

    fn rd_u32(&self, off: usize) -> u32 {
        let b = self.buf();
        let g = |i: usize| b.get(off + i).copied().unwrap_or(0);
        u32::from_ne_bytes([g(0), g(1), g(2), g(3)])
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn wr_u32(&mut self, off: usize, v: u32) {
        self.buf_mut()[off..off + 4].copy_from_slice(&v.to_ne_bytes());
    }

    /// The event most recently returned by `next_events`.
    fn current_event(&self) -> Event {
        let o = self.ret_off;
        let len = self.rd_u32(o + 12);
        let name = if len > 0 {
            let b = self.buf();
            let start = (o + EVENT_SIZE).min(b.len());
            let end = (start + len as usize).min(b.len());
            b[start..end].iter().take_while(|&&c| c != 0).copied().collect()
        } else {
            Vec::new()
        };
        Event {
            wd: self.rd_u32(o) as i32,
            mask: self.rd_u32(o + 4),
            cookie: self.rd_u32(o + 8),
            len,
            name,
        }
    }

    /// Get the next event, waiting up to `timeout` seconds (0 or negative:
    /// block).  `None` on timeout or error (see [`Inotifytools::error`]).
    pub fn next_event(&mut self, timeout: c_long) -> Option<Event> {
        let timeout = if timeout == 0 { -1 } else { timeout };
        self.next_events(timeout, 1)
    }

    /// Get the next event, waiting until roughly `num_events` events are
    /// available.  `timeout` in seconds; negative blocks.
    pub fn next_events(&mut self, timeout: c_long, num_events: c_int) -> Option<Event> {
        if self.next_events_raw(timeout, num_events) {
            Some(self.current_event())
        } else {
            None
        }
    }

    /// Advance to the next event; on success it is at `self.ret_off`.
    pub(crate) fn next_events_raw(&mut self, timeout: c_long, num_events: c_int) -> bool {
        self.assert_init();
        niceassert!(
            num_events <= MAX_EVENTS as c_int,
            "num_events <= MAX_EVENTS",
            "too many events requested"
        );

        if num_events < 1 {
            return false;
        }

        loop {
            #[allow(unused_mut)]
            let mut event_pid: pid_t = 0;
            self.error = 0;

            let mut buffered = false;
            // first_byte is index into event buffer
            if self.first_byte != 0
                && self.first_byte <= self.bytes.wrapping_sub(EVENT_SIZE as isize) as i32
            {
                self.ret_off = self.first_byte as usize;
                let len = self.rd_u32(self.ret_off + 12) as usize;
                if !self.fanotify_mode
                    && self.first_byte as usize + EVENT_SIZE + len > self.bytes as usize
                {
                    // An incomplete event (the kernel never does this).  Move
                    // what we have to the front and read the remainder.
                    let first = self.first_byte as usize;
                    let have = self.bytes as usize - first;
                    self.buf_mut().copy_within(first..first + have, 0);
                    self.bytes = have as isize;
                    self.first_byte = 0;
                } else {
                    self.this_bytes = 0;
                    buffered = true;
                }
            } else if self.first_byte == 0 {
                self.bytes = 0;
            }

            if !buffered {
                let ts = libc::timespec { tv_sec: timeout as _, tv_nsec: 0 };
                let mut pfd = libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 };
                let tsp = if timeout < 0 { ptr::null() } else { &ts as *const libc::timespec };
                let rc = unsafe { libc::ppoll(&mut pfd, 1, tsp, ptr::null()) };
                if rc < 0 {
                    self.error = errno();
                    return false;
                } else if rc == 0 {
                    // timeout
                    return false;
                }

                // wait until we have enough bytes to read
                let mut bytes_to_read: c_uint = 0;
                let mut rc;
                loop {
                    rc = unsafe { libc::ioctl(self.fd, libc::FIONREAD, &mut bytes_to_read) };
                    if rc != 0 || bytes_to_read as usize >= EVENT_SIZE * num_events as usize {
                        break;
                    }
                }
                if rc == -1 {
                    self.error = errno();
                    return false;
                }

                let cap = EVENT_SIZE * MAX_EVENTS;
                let off = (self.bytes.max(0) as usize).min(cap);
                let n = unsafe {
                    libc::read(
                        self.fd,
                        self.evbuf.as_mut_ptr().cast::<u8>().add(off).cast(),
                        cap - off,
                    )
                };
                if n < 0 {
                    self.error = errno();
                    return false;
                }
                if n == 0 {
                    ceprint!(
                        "Inotify reported end-of-file.  Possibly too many events occurred at once.\n"
                    );
                    return false;
                }
                self.this_bytes = n as isize;
            }

            // more_events:
            self.ret_off = self.first_byte as usize;
            if self.fanotify_mode {
                #[cfg(target_os = "linux")]
                if !self.convert_fanotify_event(&mut event_pid) {
                    return false;
                }
            } else {
                let len = self.rd_u32(self.ret_off + 12) as i32;
                self.first_byte += EVENT_SIZE as i32 + len;
            }

            self.bytes += self.this_bytes;
            niceassert!(
                self.first_byte as isize <= self.bytes,
                "first_byte <= bytes",
                "ridiculously long filename, things will almost certainly screw up."
            );
            if self.first_byte as isize == self.bytes {
                self.first_byte = 0;
            }

            // Skip events from self due to open_by_handle_at()
            if self.self_pid != 0 && self.self_pid == event_pid {
                continue;
            }

            // Skip a filesystem event whose path could not be resolved.
            #[cfg(target_os = "linux")]
            if self.fanotify_mark_type == fid::FAN_MARK_FILESYSTEM && self.rd_u32(self.ret_off) == 0
            {
                continue;
            }

            if self.regex.is_some() {
                let mask = self.rd_u32(self.ret_off + 4) as i32;
                // Skip regex filtering for directories in recursive mode
                if self.recursive_watch != 0
                    && mask & IN_ISDIR != 0
                    && mask & (IN_CREATE | IN_MOVED_TO) != 0
                {
                    // Allow directory events through when watching recursively
                } else {
                    let ev = self.current_event();
                    let mut match_name = NString::new();
                    self.snprintf(&mut match_name, MAX_STRLEN as c_int, &ev, Some(b"%w%f"));
                    let s = cstring(match_name.as_bytes());
                    let re = self.regex.as_ref().unwrap();
                    let matched =
                        unsafe { sys::regexec(&*re.0, s.as_ptr(), 0, ptr::null_mut(), 0) } == 0;
                    if matched != self.invert_regexp {
                        continue;
                    }
                }
            }

            if self.collect_stats {
                let (wd, mask) = (self.rd_u32(self.ret_off) as i32, self.rd_u32(self.ret_off + 4));
                self.record_stats(wd, mask);
            }

            return true;
        }
    }

    /// Convert the fanotify event at `ret_off` into an inotify event in the
    /// second half of the event buffer.
    #[cfg(target_os = "linux")]
    fn convert_fanotify_event(&mut self, event_pid: &mut pid_t) -> bool {
        let meta = self.ret_off;
        let info = meta + fid::META_LEN;
        let event_len = self.rd_u32(meta);

        self.first_byte = self.first_byte.wrapping_add(event_len as i32);

        let mut fid_off: Option<usize> = None;
        let mut name_off: Option<usize> = None;
        let mut fid_len: i32 = 0;
        let mut name_len: i32 = 0;
        if event_len as usize > fid::META_LEN {
            let b = self.buf();
            let t = b.get(info).copied().unwrap_or(0);
            if t == fid::FAN_EVENT_INFO_TYPE_FID
                || t == fid::FAN_EVENT_INFO_TYPE_DFID
                || t == fid::FAN_EVENT_INFO_TYPE_DFID_NAME
            {
                let rec = &b[info.min(b.len())..];
                fid_off = Some(info);
                let hb = fid::handle_bytes(rec) as usize;
                fid_len = (fid::FID_HDR + hb) as i32;
                if t == fid::FAN_EVENT_INFO_TYPE_DFID_NAME {
                    name_len = fid::hdr_len(rec) as i32 - fid_len;
                }
                if name_len > 0 {
                    name_off = Some(info + fid::FID_HDR + hb);
                }
                // Convert zero padding to zero name_len.  For some events on
                // directories, the fid is that of the dir and name is ".".  Do
                // not include "." name in fid hash, but keep it for debug print.
                let n0 = name_off.and_then(|o| b.get(o).copied()).unwrap_or(0);
                let n1 = name_off.and_then(|o| b.get(o + 1).copied()).unwrap_or(0);
                if name_len != 0 && (n0 == 0 || (n0 == b'.' && n1 == 0)) {
                    let newlen = (fid::hdr_len(rec) as i32 - name_len) as u16;
                    let bm = self.buf_mut();
                    bm[info + 2..info + 4].copy_from_slice(&newlen.to_ne_bytes());
                    name_len = 0;
                }
            }
        }
        let fid_off = match fid_off {
            Some(v) => v,
            None => {
                ceprint!("No fid in fanotify event.\n");
                return false;
            }
        };
        let name: Vec<u8> = match name_off {
            Some(o) => self.buf()[o.min(self.buf().len())..]
                .iter()
                .take_while(|&&c| c != 0)
                .copied()
                .collect(),
            None => Vec::new(),
        };
        if self.verbosity > 1 {
            cprint!(
                "fanotify_event: bytes=",
                self.bytes,
                ", first_byte=",
                self.first_byte,
                ", this_bytes=",
                self.this_bytes,
                ", event_len=",
                event_len,
                ", fid_len=",
                fid_len,
                ", name_len=",
                name_len,
                ", name=",
                name,
                "\n"
            );
        }

        self.ret_off = MAX_EVENTS * EVENT_SIZE;
        // btrfs fsid.val[1] differs per subvolume. Hash the event by val[0].
        {
            let b = self.buf_mut();
            let rec = &mut b[fid_off..];
            if fid::handle_type(rec) == fid::FILEID_BTRFS_WITHOUT_PARENT {
                fid::clear_fsid_val1(rec);
            }
        }
        let fid_bytes: Vec<u8> = {
            let b = self.buf();
            let hl = fid::hdr_len(&b[fid_off..]) as usize;
            b[fid_off..(fid_off + hl).min(MAX_EVENTS * EVENT_SIZE)].to_vec()
        };
        let mut wd = self.watch_from_fid(&fid_bytes).map(|id| self.watch(id).wd);
        let mask = u64::from_ne_bytes(self.buf()[meta + 8..meta + 16].try_into().unwrap());
        if wd.is_none() {
            let is_dir = mask as u32 & IN_ISDIR as u32 != 0;
            let filename = self
                .filename_from_fid(&fid_bytes, is_dir)
                .map(|p| unsafe { CStr::from_ptr(p) }.to_bytes().to_vec());
            let mut empty_path = false;
            if let Some(f) = filename.as_ref() {
                if !f.is_empty() {
                    match self.create_watch(0, Some(fid_bytes.clone()), f, 0) {
                        Some(w) => wd = Some(w),
                        None => return false,
                    }
                } else {
                    empty_path = true;
                }
            }

            // An empty path is a skipped event, so it stays quiet.
            if self.verbosity != 0 && !empty_path {
                let b = self.buf();
                let mut idb = [0u8; mem::size_of::<libc::c_ulong>()];
                for (i, x) in idb.iter_mut().enumerate() {
                    *x = b.get(fid_off + fid::FID_HDR + i).copied().unwrap_or(0);
                }
                let id = libc::c_ulong::from_ne_bytes(idb);
                let rec = &b[fid_off..];
                cprint!(
                    format!(
                        "[fid={:x}.{:x}.{:x};name='",
                        fid::fsid_val(rec, 0),
                        fid::fsid_val(rec, 1),
                        id
                    ),
                    name,
                    "'] ",
                    filename.unwrap_or_default(),
                    "\n"
                );
            }
        }

        let ret = self.ret_off;
        self.wr_u32(ret, wd.unwrap_or(0) as u32);
        self.wr_u32(ret + 4, mask as u32);
        self.wr_u32(ret + 12, name_len as u32);
        if name_len > 0 {
            let src = name_off.unwrap();
            let n = (name_len as usize).min(MAX_EVENTS * EVENT_SIZE - EVENT_SIZE);
            let b = self.buf_mut();
            for i in 0..n {
                b[ret + EVENT_SIZE + i] =
                    if src + i < MAX_EVENTS * EVENT_SIZE { b[src + i] } else { 0 };
            }
        }
        *event_pid = self.rd_u32(meta + 20) as pid_t;
        true
    }

    // ----------------------------------------------------------------
    // Regular expression filtering
    // ----------------------------------------------------------------

    fn do_ignore_events_by_regex(
        &mut self,
        pattern: Option<&[u8]>,
        flags: c_int,
        invert: bool,
        recursive: c_int,
    ) -> bool {
        let pattern = match pattern {
            Some(v) => v,
            None => {
                self.regex = None;
                return true;
            }
        };

        self.regex = None;
        self.invert_regexp = invert;
        self.recursive_watch = recursive;

        let mut re: Box<sys::regex_t> = Box::new(sys::regex_t::zeroed());
        let cpat = cstring(pattern);
        let ret = unsafe { sys::regcomp(&mut *re, cpat.as_ptr(), flags | sys::REG_NOSUB) };
        if ret == 0 {
            self.regex = Some(Regex(re));
            return true;
        }

        self.error = EINVAL;
        false
    }

    /// Ignore events on files whose path matches the POSIX regular
    /// expression `pattern` (compiled with `flags`, e.g. `REG_EXTENDED`).
    /// `None` removes the filter.  With `recursive`, directory
    /// creation/move-in events are never filtered.
    pub fn ignore_events_by_regex(
        &mut self,
        pattern: Option<&[u8]>,
        flags: c_int,
        recursive: c_int,
    ) -> bool {
        self.do_ignore_events_by_regex(pattern, flags, false, recursive)
    }

    /// Ignore events on files whose path does NOT match `pattern`.
    pub fn ignore_events_by_inverted_regex(
        &mut self,
        pattern: Option<&[u8]>,
        flags: c_int,
        recursive: c_int,
    ) -> bool {
        self.do_ignore_events_by_regex(pattern, flags, true, recursive)
    }

    // ----------------------------------------------------------------
    // Formatting
    // ----------------------------------------------------------------

    /// Convert an event mask to a `sep`-separated string, in the internal
    /// buffer (as the C API returns it).
    pub(crate) fn event_to_str_sep_ptr(&mut self, events: c_int, sep: u8) -> *mut c_char {
        const NAMES: [(i32, &[u8]); 18] = [
            (IN_ACCESS, b"ACCESS"),
            (IN_MODIFY, b"MODIFY"),
            (IN_ATTRIB, b"ATTRIB"),
            (IN_CLOSE_WRITE, b"CLOSE_WRITE"),
            (IN_CLOSE_NOWRITE, b"CLOSE_NOWRITE"),
            (IN_OPEN, b"OPEN"),
            (IN_MOVED_FROM, b"MOVED_FROM"),
            (IN_MOVED_TO, b"MOVED_TO"),
            (IN_CREATE, b"CREATE"),
            (IN_DELETE, b"DELETE"),
            (IN_DELETE_SELF, b"DELETE_SELF"),
            (IN_UNMOUNT, b"UNMOUNT"),
            (IN_Q_OVERFLOW, b"Q_OVERFLOW"),
            (IN_IGNORED, b"IGNORED"),
            (IN_CLOSE, b"CLOSE"),
            (IN_MOVE_SELF, b"MOVE_SELF"),
            (IN_ISDIR, b"ISDIR"),
            (IN_ONESHOT, b"ONESHOT"),
        ];
        let ret = &mut *self.evstr;
        let strlen = |r: &[u8]| r.iter().position(|&c| c == 0).unwrap_or(r.len());
        ret[0] = 0;
        ret[1] = 0;
        for (flag, name) in NAMES {
            if flag & events != 0 {
                // charcat(ret, sep); strncat(ret, name, ...);
                let l = strlen(&ret[..]);
                ret[l] = sep;
                ret[l + 1] = 0;
                let l = strlen(&ret[..]);
                ret[l..l + name.len()].copy_from_slice(name);
                ret[l + name.len()] = 0;
            }
        }

        // Maybe we didn't match any... ?
        if ret[0] == 0 {
            let s = format!("0x{:08x}", events as u32);
            ret[0] = sep;
            ret[1..1 + s.len()].copy_from_slice(s.as_bytes());
            ret[1 + s.len()] = 0;
        }

        unsafe { ret.as_mut_ptr().add(1).cast() }
    }

    /// Convert an event mask to a string of event names separated by `sep`.
    /// Unknown masks are rendered in hexadecimal.
    pub fn event_to_str_sep(&mut self, events: c_int, sep: u8) -> Vec<u8> {
        let p = self.event_to_str_sep_ptr(events, sep);
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }

    /// Convert an event mask to a comma-separated string of event names.
    pub fn event_to_str(&mut self, events: c_int) -> Vec<u8> {
        self.event_to_str_sep(events, b',')
    }

    /// Set the strftime format used for `%T`.
    pub fn set_printf_timefmt(&mut self, fmt: &[u8]) {
        self.timefmt = Some(cstring(fmt).into_bytes());
    }

    /// Stop substituting `%T`.
    pub fn clear_timefmt(&mut self) {
        self.timefmt = None;
    }

    /// Construct a string from an event using a printf-like format (`%w`,
    /// `%f`, `%e`, `%Xe`, `%c`, `%T`, `%0`, `%n`, `%%`), writing at most
    /// `size` characters.  Returns the number of characters written minus
    /// one, or -1 on error (C API compatible).
    pub fn snprintf(
        &mut self,
        out: &mut NString,
        size: c_int,
        ev: &Event,
        fmt: Option<&[u8]>,
    ) -> c_int {
        let path = self.filename_from_event(ev);
        let filename = &path.filename;
        let dirnamelen = path.dirnamelen;
        let eventname = &path.eventname;

        let fmt = match fmt {
            Some(f) if !f.is_empty() => f,
            _ => {
                self.error = EINVAL;
                return -1;
            }
        };
        if fmt.len() > MAX_STRLEN || size > MAX_STRLEN as c_int {
            self.error = EMSGSIZE;
            return -1;
        }

        let size = size as i64;
        let flen = fmt.len();
        let mut ind: u32 = 0;
        let mut i: usize = 0;
        while i < flen && (ind as i64) < size - 1 {
            if fmt[i] != b'%' {
                out.buf[ind as usize] = fmt[i];
                ind += 1;
                i += 1;
                continue;
            }

            if i == flen - 1 {
                // last character is %, invalid
                self.error = EINVAL;
                return ind as c_int;
            }

            let ch1 = fmt[i + 1];
            let room = (size - ind as i64) as usize;

            match ch1 {
                b'%' | b'0' | b'n' => {
                    out.buf[ind as usize] = match ch1 {
                        b'%' => b'%',
                        b'0' => 0,
                        _ => b'\n',
                    };
                    ind += 1;
                }
                b'w' => {
                    if dirnamelen <= room {
                        strncpy(&mut out.buf, ind, filename, dirnamelen);
                        ind += dirnamelen as u32;
                    }
                }
                b'f' => {
                    strncpy(&mut out.buf, ind, eventname, room);
                    ind += eventname.len() as u32;
                }
                b'c' => {
                    let s = format!("{:x}", ev.cookie);
                    // snprintf(&buf[ind], room, "%x", cookie)
                    let n = s.len().min(room - 1);
                    let start = ind as usize;
                    out.buf[start..start + n].copy_from_slice(&s.as_bytes()[..n]);
                    out.buf[start + n] = 0;
                    ind += s.len() as u32;
                }
                b'e' => {
                    let s = self.event_to_str_sep(ev.mask as c_int, b',');
                    strncpy(&mut out.buf, ind, &s, room);
                    ind += s.len() as u32;
                }
                b'T' => {
                    let timestr = if let Some(tf) = &self.timefmt {
                        match strftime_now(tf) {
                            Some(t) => t,
                            None => {
                                // time format probably invalid
                                self.error = EINVAL;
                                return ind as c_int;
                            }
                        }
                    } else {
                        Vec::new()
                    };
                    strncpy(&mut out.buf, ind, &timestr, room);
                    ind += timestr.len() as u32;
                }
                _ => {
                    // Check if next char in fmt is e
                    if i < flen - 2 && fmt[i + 2] == b'e' {
                        let s = self.event_to_str_sep(ev.mask as c_int, ch1);
                        strncpy(&mut out.buf, ind, &s, room);
                        ind += s.len() as u32;
                        i += 3;
                        continue;
                    }

                    // OK, this wasn't a special format character, just
                    // output it as normal
                    if (ind as usize) < MAX_STRLEN {
                        out.buf[ind as usize] = b'%';
                        ind += 1;
                    }
                    if (ind as usize) < MAX_STRLEN {
                        out.buf[ind as usize] = ch1;
                        ind += 1;
                    }
                }
            }
            i += 2;
        }
        out.len = ind;

        ind.wrapping_sub(1) as c_int
    }

    /// Like [`Inotifytools::snprintf`] with `size` = `MAX_STRLEN`.
    pub fn sprintf(&mut self, out: &mut NString, ev: &Event, fmt: Option<&[u8]>) -> c_int {
        self.snprintf(out, MAX_STRLEN as c_int, ev, fmt)
    }

    /// Format an event and write it to a C stream.
    ///
    /// # Safety
    /// `file` must be NULL or a valid, open C `FILE` stream.
    pub unsafe fn fprintf_to(
        &mut self,
        file: *mut libc::FILE,
        ev: &Event,
        fmt: Option<&[u8]>,
    ) -> c_int {
        let mut out = self.print_out.take().unwrap_or_else(NString::new);
        let ret = self.sprintf(&mut out, ev, fmt);
        if ret != -1 {
            crate::cio::write_to(file, out.as_bytes());
        }
        self.print_out = Some(out);
        ret
    }

    /// Format an event and write it to (C) standard output.
    pub fn printf(&mut self, ev: &Event, fmt: Option<&[u8]>) -> c_int {
        unsafe { self.fprintf_to(crate::cio::c_stdout(), ev, fmt) }
    }

    // ----------------------------------------------------------------
    // Statistics
    // ----------------------------------------------------------------

    fn record_stats(&mut self, wd: i32, mask: u32) {
        let id = match self.watch_from_wd(wd) {
            Some(v) => v,
            None => {
                return;
            }
        };
        if let Some(w) = self.watches.get_mut(&id) {
            w.stats.record(mask);
        }
        self.totals.record(mask);
    }

    /// Initialise or reset statistics collection.
    pub fn initialize_stats(&mut self) {
        self.assert_init();

        // if already collecting stats, reset stats
        if self.collect_stats {
            let ids: Vec<u64> = self.by_wd.values().copied().collect();
            for id in ids {
                if let Some(w) = self.watches.get_mut(&id) {
                    w.stats = WatchStats::default();
                }
            }
        }
        self.totals = WatchStats::default();
        self.collect_stats = true;
    }

    /// Number of occurrences of `event` (0 for total) on watch `wd`, or -1.
    pub fn get_stat_by_wd(&self, wd: i32, event: c_int) -> c_int {
        if !self.collect_stats {
            return -1;
        }
        let id = match self.watch_from_wd(wd) {
            Some(v) => v,
            None => {
                return -1;
            }
        };
        match self.watch(id).stats.get(event) {
            Some(v) => v as c_int,
            None => -1,
        }
    }

    /// Number of occurrences of `event` (0 for total) over all watches, or
    /// -1.
    pub fn get_stat_total(&self, event: c_int) -> c_int {
        if !self.collect_stats {
            return -1;
        }
        match self.totals.get(event) {
            Some(v) => v as c_int,
            None => -1,
        }
    }

    /// Number of occurrences of `event` on the watch for `filename`, or -1.
    pub fn get_stat_by_filename(&self, filename: &[u8], event: c_int) -> c_int {
        self.get_stat_by_wd(self.wd_from_filename(filename), event)
    }

    /// All watches (by watch descriptor) with their statistics, sorted by
    /// an event's count: `sort_event` > 0 ascending by that event, 0
    /// ascending by total, -1 descending by total, other negative values
    /// descending by `-sort_event`.  Ties are broken by watch descriptor.
    /// Use [`Inotifytools::filename_from_watch`] to get a watch's filename.
    pub fn watches_sorted_by_event(&self, sort_event: c_int) -> Vec<(WatchRef, WatchStats)> {
        let (event, asc) = if sort_event == -1 {
            (0, false)
        } else if sort_event < 0 {
            (sort_event.wrapping_neg(), false)
        } else {
            (sort_event, true)
        };
        let mut ids: Vec<u64> = self.by_wd.values().copied().collect();
        ids.sort_by(|a, b| {
            let (wa, wb) = (self.watch(*a), self.watch(*b));
            let i1 = wa.stats.get(event).unwrap_or(0);
            let i2 = wb.stats.get(event).unwrap_or(0);
            if i1 == i2 {
                wa.wd.cmp(&wb.wd)
            } else if asc {
                i1.cmp(&i2)
            } else {
                i2.cmp(&i1)
            }
        });
        ids.into_iter().map(|id| (WatchRef(id), self.watch(id).stats)).collect()
    }

    /// The filename of a watch (resolving fanotify filesystem watch fids),
    /// or empty if the watch no longer exists.
    pub fn filename_from_watch(&mut self, w: WatchRef) -> Vec<u8> {
        if !self.watches.contains_key(&w.0) {
            return Vec::new();
        }
        let p = self.filename_from_watch_id(w.0);
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }

    // ----------------------------------------------------------------
    // Kernel limits
    // ----------------------------------------------------------------

    fn read_num_from_file(&mut self, path: &[u8]) -> Option<c_int> {
        unsafe {
            let f = libc::fopen(path.as_ptr().cast(), b"r\0".as_ptr().cast());
            if f.is_null() {
                self.error = errno();
                return None;
            }
            let mut num: c_int = 0;
            if libc::fscanf(f, b"%d\0".as_ptr().cast(), &mut num as *mut c_int) == libc::EOF {
                self.error = errno();
                let fclose_ret = libc::fclose(f);
                niceassert!(fclose_ret == 0, "!fclose_ret", "");
                return None;
            }
            let fclose_ret = libc::fclose(f);
            niceassert!(fclose_ret == 0, "!fclose_ret", "");
            Some(num)
        }
    }

    /// The maximum number of events queued in the kernel, or -1.
    pub fn get_max_queued_events(&mut self) -> c_int {
        self.read_num_from_file(QUEUE_SIZE_PATH).unwrap_or(-1)
    }

    /// The maximum number of inotify instances per user, or -1.
    pub fn get_max_user_instances(&mut self) -> c_int {
        self.read_num_from_file(INSTANCES_PATH).unwrap_or(-1)
    }

    /// The maximum number of inotify watches per user, or -1.
    pub fn get_max_user_watches(&mut self) -> c_int {
        self.read_num_from_file(WATCHES_SIZE_PATH).unwrap_or(-1)
    }
}

/// `strncpy(&buf[ind], src, n)`: copy `src` and zero-fill up to `n` bytes.
fn strncpy(buf: &mut [u8; MAX_STRLEN], ind: u32, src: &[u8], n: usize) {
    let start = ind as usize;
    if start >= MAX_STRLEN {
        return;
    }
    let end = start.saturating_add(n).min(MAX_STRLEN);
    for (k, slot) in buf[start..end].iter_mut().enumerate() {
        *slot = src.get(k).copied().unwrap_or(0);
    }
}

/// `strftime()` of the current local time; `None` if the result is empty
/// (which the original treats as an invalid format).
fn strftime_now(fmt: &[u8]) -> Option<Vec<u8>> {
    let cfmt = cstring(fmt);
    let mut buf = vec![0u8; MAX_STRLEN];
    unsafe {
        let now = libc::time(ptr::null_mut());
        let mut tm: libc::tm = mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let n = libc::strftime(buf.as_mut_ptr().cast(), MAX_STRLEN - 1, cfmt.as_ptr(), &tm);
        if n == 0 {
            return None;
        }
        buf.truncate(n);
    }
    Some(buf)
}

fn is_excluded(next_file: &[u8], exclude_list: &[Vec<u8>]) -> bool {
    exclude_list.iter().any(|ex| {
        let mut l = ex.len();
        if l > 0 && ex[l - 1] == b'/' {
            l -= 1;
        }
        next_file.len() == l + 1 && ex[..l] == next_file[..l]
    })
}

/// Whether `path` is a directory (not following symlinks).  Prints a
/// diagnostic for errors other than `ENOENT`.
pub fn isdir(path: &[u8]) -> bool {
    let cpath = cstring(path);
    let mut st: libc::stat = unsafe { mem::zeroed() };
    if unsafe { libc::lstat(cpath.as_ptr(), &mut st) } == -1 {
        let e = errno();
        if e != ENOENT {
            ceprint!("Stat failed on ", cpath, ": ", strerror(e), "\n");
        }
        return false;
    }
    st.st_mode & libc::S_IFMT == libc::S_IFDIR
}

/// Convert a single event name (case insensitive, without `IN_`) to its
/// mask; 0 for empty, -1 for unknown.
fn onestr_to_event(event: &[u8]) -> c_int {
    if event.is_empty() {
        return 0;
    }
    const TABLE: [(&[u8], i32); 20] = [
        (b"ACCESS", IN_ACCESS),
        (b"MODIFY", IN_MODIFY),
        (b"ATTRIB", IN_ATTRIB),
        (b"CLOSE_WRITE", IN_CLOSE_WRITE),
        (b"CLOSE_NOWRITE", IN_CLOSE_NOWRITE),
        (b"OPEN", IN_OPEN),
        (b"MOVED_FROM", IN_MOVED_FROM),
        (b"MOVED_TO", IN_MOVED_TO),
        (b"CREATE", IN_CREATE),
        (b"DELETE", IN_DELETE),
        (b"DELETE_SELF", IN_DELETE_SELF),
        (b"UNMOUNT", IN_UNMOUNT),
        (b"Q_OVERFLOW", IN_Q_OVERFLOW),
        (b"IGNORED", IN_IGNORED),
        (b"CLOSE", IN_CLOSE),
        (b"MOVE_SELF", IN_MOVE_SELF),
        (b"MOVE", IN_MOVE),
        (b"ISDIR", IN_ISDIR),
        (b"ONESHOT", IN_ONESHOT),
        (b"ALL_EVENTS", IN_ALL_EVENTS),
    ];
    TABLE.iter().find(|(name, _)| name.eq_ignore_ascii_case(event)).map(|&(_, v)| v).unwrap_or(-1)
}

/// Convert `sep`-separated event names to a mask.  Returns 0 if `event` is
/// empty or contains an empty element, -1 if an element is unknown or `sep`
/// is a letter, '_' or NUL.
pub fn str_to_event_sep(event: Option<&[u8]>, sep: u8) -> c_int {
    if sep == 0 || sep == b'_' || sep.is_ascii_alphabetic() {
        return -1;
    }

    const EVENTSTR_SIZE: usize = 4096;
    let event = match event {
        Some(v) => v,
        None => return 0,
    };
    // Treat the input as a C string.
    let event = &event[..event.iter().position(|&c| c == 0).unwrap_or(event.len())];
    if event.is_empty() {
        return 0;
    }

    let find = |s: &[u8]| s.iter().position(|&c| c == sep);
    let mut ret: c_int = 0;
    let mut event1: &[u8] = event;
    let mut event2 = find(event1);
    while !event1.is_empty() {
        let mut len = match event2 {
            Some(p) => {
                niceassert!(
                    p < EVENTSTR_SIZE,
                    "len < eventstr_size",
                    "malformed event string (very long)"
                );
                p
            }
            None => event1.len(),
        };
        if len > EVENTSTR_SIZE - 1 {
            len = EVENTSTR_SIZE - 1;
        }

        let ret1 = onestr_to_event(&event1[..len]);
        if ret1 == 0 || ret1 == -1 {
            ret = ret1;
            break;
        }
        ret |= ret1;

        match event2 {
            None => break,
            Some(p) => {
                // jump over 'sep' character
                event1 = &event1[p + 1..];
                // if last character was 'sep'...
                if event1.is_empty() {
                    return 0;
                }
                event2 = find(event1);
            }
        }
    }

    ret
}

/// Convert comma-separated event names to a mask; see [`str_to_event_sep`].
pub fn str_to_event(event: Option<&[u8]>) -> c_int {
    str_to_event_sep(event, b',')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_to_event_matches_c_semantics() {
        let e = |s: &str| str_to_event(Some(s.as_bytes()));
        assert_eq!(e("open,modify,access"), IN_OPEN | IN_MODIFY | IN_ACCESS);
        assert_eq!(e(",open,modify,access"), 0);
        assert_eq!(e("open,modify,access,"), 0);
        assert_eq!(e("open,modify,,access,close"), 0);
        assert_eq!(e("open,mod,access,close"), -1);
        assert_eq!(e("CLOSE"), IN_CLOSE);
        assert_eq!(e(""), 0);
        assert_eq!(str_to_event(None), 0);
        assert_eq!(str_to_event_sep(Some(b"open:modify"), b':'), IN_OPEN | IN_MODIFY);
        assert_eq!(str_to_event_sep(Some(b"open:modify"), b'o'), -1);
        assert_eq!(str_to_event_sep(Some(b"open:modify"), 0), -1);
    }

    #[test]
    fn event_to_str_order_and_unknown() {
        let mut lib = Inotifytools::new();
        assert_eq!(lib.event_to_str(IN_OPEN | IN_MODIFY | IN_ACCESS), b"ACCESS,MODIFY,OPEN");
        assert_eq!(lib.event_to_str_sep(IN_CLOSE_WRITE, b'.'), b"CLOSE_WRITE.CLOSE");
        assert_eq!(lib.event_to_str(0x1000), b"0x00001000");
        assert_eq!(lib.event_to_str(0), b"0x00000000");
    }

    #[test]
    fn snprintf_formats_and_truncates() {
        let dir = std::env::temp_dir().join(format!("inotifytools_rs_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dirb = dir.to_str().unwrap().as_bytes().to_vec();
        let mut lib = Inotifytools::new();
        assert!(lib.initialize());
        if !lib.watch_file(&dirb, IN_CLOSE) {
            // Don't fail on a machine whose inotify watches are exhausted.
            assert_eq!(lib.error(), libc::ENOSPC);
            eprintln!("skipping: inotify watch limit reached");
            return;
        }
        let watched = cat!(dirb, "/");
        let wd = lib.wd_from_filename(&watched);
        assert!(wd > 0);
        assert_eq!(lib.filename_from_wd(wd), watched);

        let mut out = NString::new();
        let mut ev = Event { wd, mask: IN_ACCESS as u32, ..Default::default() };
        lib.snprintf(&mut out, MAX_STRLEN as c_int, &ev, Some(b"Event %e %.e on %w %f %T"));
        assert_eq!(out.as_bytes(), cat!("Event ACCESS ACCESS on ", watched, "  "));

        ev.mask = IN_MODIFY as u32;
        let mut out = NString::new();
        lib.snprintf(&mut out, 10, &ev, Some(b"Event %e %.e on %w %f %T"));
        assert_eq!(&out.buf[..10], b"Event MODI");

        ev.name = b"my_great_file".to_vec();
        ev.len = 14;
        ev.mask = IN_ACCESS as u32;
        let mut out = NString::new();
        lib.snprintf(&mut out, MAX_STRLEN as c_int, &ev, Some(b"%w%f|%c|%%|%n"));
        assert_eq!(out.as_bytes(), cat!(watched, "my_great_file|0|%|\n"));

        let mut out = NString::new();
        assert_eq!(lib.snprintf(&mut out, 100, &ev, Some(b"")), -1);
        assert_eq!(lib.error(), EINVAL);

        assert_eq!(lib.get_num_watches(), 1);
        assert!(lib.remove_watch_by_filename(&watched));
        assert_eq!(lib.get_num_watches(), 0);
        assert_eq!(lib.wd_from_filename(&watched), -1);
        lib.cleanup();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
