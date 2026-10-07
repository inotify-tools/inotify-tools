//! libinotifytools: a small library simplifying the use of inotify and
//! fanotify.
//!
//! Use it from Rust through [`Inotifytools`], or from C through the
//! cdylib/staticlib, which implements `inotifytools/inotifytools.h` with the
//! same ABI and behaviour as the original C library.

#![allow(clippy::missing_safety_doc)]

pub mod consts;
#[doc(hidden)]
#[macro_use]
pub mod cio;
mod ffi;
#[cfg(target_os = "linux")]
mod fid;
mod inotify;
#[doc(hidden)]
pub mod sys;

pub use inotify::{
    isdir, str_to_event, str_to_event_sep, Event, EventPath, Inotifytools, NString, WatchRef,
    WatchScope, WatchStats, MAX_STRLEN,
};
