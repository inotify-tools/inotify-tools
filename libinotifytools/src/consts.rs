//! inotify event constants (same values as `<sys/inotify.h>`), as C `int`s
//! because that is how the libinotifytools API passes event masks around.

pub const IN_ACCESS: i32 = 0x0000_0001;
pub const IN_MODIFY: i32 = 0x0000_0002;
pub const IN_ATTRIB: i32 = 0x0000_0004;
pub const IN_CLOSE_WRITE: i32 = 0x0000_0008;
pub const IN_CLOSE_NOWRITE: i32 = 0x0000_0010;
pub const IN_CLOSE: i32 = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;
pub const IN_OPEN: i32 = 0x0000_0020;
pub const IN_MOVED_FROM: i32 = 0x0000_0040;
pub const IN_MOVED_TO: i32 = 0x0000_0080;
pub const IN_MOVE: i32 = IN_MOVED_FROM | IN_MOVED_TO;
pub const IN_CREATE: i32 = 0x0000_0100;
pub const IN_DELETE: i32 = 0x0000_0200;
pub const IN_DELETE_SELF: i32 = 0x0000_0400;
pub const IN_MOVE_SELF: i32 = 0x0000_0800;
pub const IN_UNMOUNT: i32 = 0x0000_2000;
pub const IN_Q_OVERFLOW: i32 = 0x0000_4000;
pub const IN_IGNORED: i32 = 0x0000_8000;
pub const IN_ONLYDIR: i32 = 0x0100_0000;
pub const IN_DONT_FOLLOW: i32 = 0x0200_0000;
pub const IN_EXCL_UNLINK: i32 = 0x0400_0000;
pub const IN_MASK_ADD: i32 = 0x2000_0000;
pub const IN_ISDIR: i32 = 0x4000_0000;
pub const IN_ONESHOT: i32 = 0x8000_0000_u32 as i32;
/// `IN_ALL_EVENTS` without the self events. A filesystem watch already
/// reports delete and move on the parent directory.
pub const FS_ALL_EVENTS: i32 = IN_ALL_EVENTS & !(IN_DELETE_SELF | IN_MOVE_SELF);
/// Legacy fanotify events. A mount mark does not report create, delete, or move.
pub const FAN_ALL_EVENTS: i32 = IN_ACCESS | IN_MODIFY | IN_CLOSE | IN_OPEN;
pub const IN_ALL_EVENTS: i32 = IN_ACCESS
    | IN_MODIFY
    | IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_CLOSE_NOWRITE
    | IN_OPEN
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_CREATE
    | IN_DELETE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;
