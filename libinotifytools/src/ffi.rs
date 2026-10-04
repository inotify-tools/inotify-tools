//! The C ABI of `inotifytools/inotifytools.h`.  Like the original it uses
//! one process-global, never freed instance and is not thread-safe.

use std::cell::UnsafeCell;
use std::ffi::CStr;
use std::ptr;

use libc::{c_char, c_int, c_long, c_void, size_t, FILE};

use crate::sys::inotify_event;

use crate::inotify::{self, Event, Inotifytools, NString, WatchScope, EMPTY, MAX_STRLEN};

struct Global(UnsafeCell<Option<Inotifytools>>);
// SAFETY: not thread-safe, exactly like the original library's globals.
unsafe impl Sync for Global {}

static GLOBAL: Global = Global(UnsafeCell::new(None));

#[allow(clippy::mut_from_ref)]
fn lib() -> &'static mut Inotifytools {
    unsafe { (*GLOBAL.0.get()).get_or_insert_with(Inotifytools::new) }
}

unsafe fn opt_bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    if s.is_null() {
        None
    } else {
        Some(CStr::from_ptr(s).to_bytes())
    }
}

unsafe fn bytes<'a>(s: *const c_char) -> &'a [u8] {
    opt_bytes(s).unwrap_or(b"")
}

/// Copy a C `struct inotify_event` (name read as a C string when len > 0).
unsafe fn event_from_c(ev: *const inotify_event) -> Event {
    if ev.is_null() {
        return Event::default();
    }
    let e = &*ev;
    let name =
        if e.len > 0 { CStr::from_ptr(event_name_ptr(ev)).to_bytes().to_vec() } else { Vec::new() };
    Event { wd: e.wd, mask: e.mask, cookie: e.cookie, len: e.len, name }
}

unsafe fn event_name_ptr(ev: *const inotify_event) -> *const c_char {
    (ev as *const u8).add(std::mem::size_of::<inotify_event>()) as *const c_char
}

unsafe fn str_list(list: *const *const c_char) -> Vec<Vec<u8>> {
    let mut v = Vec::new();
    if list.is_null() {
        return v;
    }
    let mut p = list;
    while !(*p).is_null() {
        v.push(CStr::from_ptr(*p).to_bytes().to_vec());
        p = p.add(1);
    }
    v
}

unsafe fn malloc_cstr(b: &[u8]) -> *mut c_char {
    let p = libc::malloc(b.len() + 1) as *mut u8;
    if p.is_null() {
        return ptr::null_mut();
    }
    ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
    *p.add(b.len()) = 0;
    p as *mut c_char
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_str_to_event(event: *const c_char) -> c_int {
    inotify::str_to_event(opt_bytes(event))
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_str_to_event_sep(event: *const c_char, sep: c_char) -> c_int {
    inotify::str_to_event_sep(opt_bytes(event), sep as u8)
}

#[no_mangle]
pub extern "C" fn inotifytools_event_to_str(events: c_int) -> *mut c_char {
    lib().event_to_str_sep_ptr(events, b',')
}

#[no_mangle]
pub extern "C" fn inotifytools_event_to_str_sep(events: c_int, sep: c_char) -> *mut c_char {
    lib().event_to_str_sep_ptr(events, sep as u8)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_set_filename_by_wd(wd: c_int, filename: *const c_char) {
    if filename.is_null() {
        return;
    }
    lib().set_filename_by_wd(wd, bytes(filename));
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_set_filename_by_filename(
    oldname: *const c_char,
    newname: *const c_char,
) {
    if oldname.is_null() || newname.is_null() {
        return;
    }
    lib().set_filename_by_filename(bytes(oldname), bytes(newname));
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_replace_filename(
    oldname: *const c_char,
    newname: *const c_char,
) {
    if oldname.is_null() || newname.is_null() {
        return;
    }
    lib().replace_filename(bytes(oldname), bytes(newname));
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_dirname_from_event(
    event: *mut inotify_event,
    dirnamelen: *mut size_t,
) -> *const c_char {
    let ev = event_from_c(event);
    let (f, dl) = lib().dirname_from_event_ptr(ev.wd);
    if !dirnamelen.is_null() {
        *dirnamelen = dl;
    }
    f
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_filename_from_event(
    event: *mut inotify_event,
    eventname: *mut *const c_char,
    dirnamelen: *mut size_t,
) -> *const c_char {
    let ev = event_from_c(event);
    let mut name = if !event.is_null() && ev.len > 0 { event_name_ptr(event) } else { EMPTY };
    let (f, dl, off) = lib().filename_from_event_ptr(ev.wd);
    if let Some(o) = off {
        name = f.add(o);
    }
    if !dirnamelen.is_null() {
        *dirnamelen = dl;
    }
    if !eventname.is_null() {
        *eventname = name;
    }
    f
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_dirpath_from_event(event: *mut inotify_event) -> *mut c_char {
    let ev = event_from_c(event);
    match lib().dirpath_from_event(&ev) {
        Some(p) => malloc_cstr(&p),
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_filename_from_watch(w: *mut c_void) -> *const c_char {
    lib().filename_from_watch_ptr(w as *const _)
}

#[no_mangle]
pub extern "C" fn inotifytools_filename_from_wd(wd: c_int) -> *const c_char {
    lib().filename_from_wd_ptr(wd)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_wd_from_filename(filename: *const c_char) -> c_int {
    lib().wd_from_filename(bytes(filename))
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_remove_watch_by_filename(filename: *const c_char) -> c_int {
    lib().remove_watch_by_filename(bytes(filename)) as c_int
}

#[no_mangle]
pub extern "C" fn inotifytools_remove_watch_by_wd(wd: c_int) -> c_int {
    lib().remove_watch_by_wd(wd) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_watch_file(filename: *const c_char, events: c_int) -> c_int {
    let list: [*const c_char; 2] = [filename, ptr::null()];
    inotifytools_watch_files(list.as_ptr(), events)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_watch_files(
    filenames: *const *const c_char,
    events: c_int,
) -> c_int {
    let owned = str_list(filenames);
    let refs: Vec<&[u8]> = owned.iter().map(|v| v.as_slice()).collect();
    lib().watch_files(&refs, events) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_watch_recursively(
    path: *const c_char,
    events: c_int,
) -> c_int {
    inotifytools_watch_recursively_with_exclude(path, events, ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_watch_recursively_with_exclude(
    path: *const c_char,
    events: c_int,
    exclude_list: *const *const c_char,
) -> c_int {
    let l = lib();
    if path.is_null() {
        l.watch_recursively_with_exclude(b"", events, &[]);
        return 0;
    }
    let excl = str_list(exclude_list);
    l.watch_recursively_with_exclude(bytes(path), events, &excl) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_ignore_events_by_regex(
    pattern: *const c_char,
    flags: c_int,
    recursive: c_int,
) -> c_int {
    lib().ignore_events_by_regex(opt_bytes(pattern), flags, recursive) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_ignore_events_by_inverted_regex(
    pattern: *const c_char,
    flags: c_int,
    recursive: c_int,
) -> c_int {
    lib().ignore_events_by_inverted_regex(opt_bytes(pattern), flags, recursive) as c_int
}

#[no_mangle]
pub extern "C" fn inotifytools_next_event(timeout: c_long) -> *mut inotify_event {
    let timeout = if timeout == 0 { -1 } else { timeout };
    inotifytools_next_events(timeout, 1)
}

#[no_mangle]
pub extern "C" fn inotifytools_next_events(
    timeout: c_long,
    num_events: c_int,
) -> *mut inotify_event {
    let l = lib();
    if l.next_events_raw(timeout, num_events) {
        l.event_ptr()
    } else {
        ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn inotifytools_error() -> c_int {
    lib().error()
}

#[no_mangle]
pub extern "C" fn inotifytools_get_stat_by_wd(wd: c_int, event: c_int) -> c_int {
    lib().get_stat_by_wd(wd, event)
}

#[no_mangle]
pub extern "C" fn inotifytools_get_stat_total(event: c_int) -> c_int {
    lib().get_stat_total(event)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_get_stat_by_filename(
    filename: *const c_char,
    event: c_int,
) -> c_int {
    lib().get_stat_by_filename(bytes(filename), event)
}

#[no_mangle]
pub extern "C" fn inotifytools_initialize_stats() {
    lib().initialize_stats()
}

#[no_mangle]
pub extern "C" fn inotifytools_initialize() -> c_int {
    lib().initialize() as c_int
}

#[no_mangle]
pub extern "C" fn inotifytools_init(
    fanotify: c_int,
    watch_filesystem: c_int,
    verbose: c_int,
) -> c_int {
    let scope = if watch_filesystem != 0 { WatchScope::Filesystem } else { WatchScope::Inode };
    lib().init(fanotify != 0, scope, verbose) as c_int
}

#[no_mangle]
pub extern "C" fn inotifytools_cleanup() {
    lib().cleanup()
}

#[no_mangle]
pub extern "C" fn inotifytools_get_num_watches() -> c_int {
    lib().get_num_watches()
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_printf(
    event: *mut inotify_event,
    fmt: *const c_char,
) -> c_int {
    inotifytools_fprintf(crate::cio::c_stdout(), event, fmt)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_fprintf(
    file: *mut FILE,
    event: *mut inotify_event,
    fmt: *const c_char,
) -> c_int {
    let ev = event_from_c(event);
    lib().fprintf_to(file, &ev, opt_bytes(fmt))
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_sprintf(
    out: *mut NString,
    event: *mut inotify_event,
    fmt: *const c_char,
) -> c_int {
    inotifytools_snprintf(out, MAX_STRLEN as c_int, event, fmt)
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_snprintf(
    out: *mut NString,
    size: c_int,
    event: *mut inotify_event,
    fmt: *const c_char,
) -> c_int {
    if out.is_null() {
        return -1;
    }
    let ev = event_from_c(event);
    lib().snprintf(&mut *out, size, &ev, opt_bytes(fmt))
}

#[no_mangle]
pub unsafe extern "C" fn inotifytools_set_printf_timefmt(fmt: *const c_char) {
    // The original formatted this with asprintf("%s", fmt), which renders
    // NULL as "(null)".
    lib().set_printf_timefmt(opt_bytes(fmt).unwrap_or(b"(null)"));
}

#[no_mangle]
pub extern "C" fn inotifytools_clear_timefmt() {
    lib().clear_timefmt()
}

#[no_mangle]
pub extern "C" fn inotifytools_get_max_user_watches() -> c_int {
    lib().get_max_user_watches()
}

#[no_mangle]
pub extern "C" fn inotifytools_get_max_user_instances() -> c_int {
    lib().get_max_user_instances()
}

#[no_mangle]
pub extern "C" fn inotifytools_get_max_queued_events() -> c_int {
    lib().get_max_queued_events()
}
