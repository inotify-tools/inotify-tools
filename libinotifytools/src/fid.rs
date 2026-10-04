//! Byte-level helpers for fanotify file identifiers.
//!
//! A "fid" here is the raw bytes of what the C code called
//! `struct fanotify_event_fid`:
//!
//! ```text
//! offset  size  field
//!      0     1  info.hdr.info_type
//!      1     1  info.hdr.pad
//!      2     2  info.hdr.len        (length of the whole record)
//!      4     8  info.fsid           (two 32-bit ints)
//!     12     4  handle.handle_bytes
//!     16     4  handle.handle_type
//!     20     *  handle.f_handle     (handle_bytes bytes, then an optional
//!                                    NUL-terminated name for DFID_NAME)
//! ```
//!
//! i.e. a kernel `fanotify_event_info_fid` record.  Watches are keyed by the
//! first `hdr.len` bytes, ordered by length then bytes, like the original.

pub const FAN_REPORT_FID: u32 = 0x0000_0200;
pub const FAN_REPORT_DFID_NAME: u32 = 0x0000_0400 | 0x0000_0800;
pub const FAN_MARK_ADD: u32 = 0x0000_0001;
pub const FAN_MARK_DONT_FOLLOW: u32 = 0x0000_0004;
pub const FAN_MARK_INODE: u32 = 0x0000_0000;
pub const FAN_MARK_FILESYSTEM: u32 = 0x0000_0100;
pub const FAN_EVENT_ON_CHILD: i32 = 0x0800_0000;

pub const FAN_EVENT_INFO_TYPE_FID: u8 = 1;
pub const FAN_EVENT_INFO_TYPE_DFID_NAME: u8 = 2;
pub const FAN_EVENT_INFO_TYPE_DFID: u8 = 3;

/// `sizeof(struct fanotify_event_fid)`
pub const FID_HDR: usize = 20;
/// Offset of the `struct file_handle` inside a fid.
pub const HANDLE_OFF: usize = 12;
/// Maximum file handle size (`MAX_HANDLE_SZ` in libc).
pub const MAX_FID_LEN: usize = 128;
/// `AT_HANDLE_FID` (same value as `AT_REMOVEDIR`). Glibc headers on older
/// systems do not define it. Overlayfs can encode a file handle with this
/// flag since Linux 6.6, which is enough for an inode watch.
pub const AT_HANDLE_FID: i32 = 0x200;
/// `BTRFS_SUPER_MAGIC` from `linux/magic.h`.
pub const BTRFS_SUPER_MAGIC: i64 = 0x9123_683E;
/// `FILEID_BTRFS_WITHOUT_PARENT` from `linux/exportfs.h`.
pub const FILEID_BTRFS_WITHOUT_PARENT: u32 = 0x4d;
/// `sizeof(struct fanotify_event_metadata)`
pub const META_LEN: usize = 24;

fn get(f: &[u8], off: usize) -> u8 {
    f.get(off).copied().unwrap_or(0)
}

pub fn info_type(f: &[u8]) -> u8 {
    get(f, 0)
}

pub fn set_info_type(f: &mut [u8], t: u8) {
    f[0] = t;
}

pub fn hdr_len(f: &[u8]) -> u16 {
    u16::from_ne_bytes([get(f, 2), get(f, 3)])
}

pub fn set_hdr_len(f: &mut [u8], len: u16) {
    f[2..4].copy_from_slice(&len.to_ne_bytes());
}

pub fn fsid_val(f: &[u8], i: usize) -> u32 {
    let o = 4 + 4 * i;
    u32::from_ne_bytes([get(f, o), get(f, o + 1), get(f, o + 2), get(f, o + 3)])
}

pub fn handle_type(f: &[u8]) -> u32 {
    u32::from_ne_bytes([get(f, 16), get(f, 17), get(f, 18), get(f, 19)])
}

/// btrfs `fsid.val[1]` differs per subvolume, so hash by `val[0]` only.
pub fn clear_fsid_val1(f: &mut [u8]) {
    if f.len() >= 12 {
        f[8..12].fill(0);
    }
}

pub fn handle_bytes(f: &[u8]) -> u32 {
    u32::from_ne_bytes([get(f, 12), get(f, 13), get(f, 14), get(f, 15)])
}

/// Lookup key for a fid: `(hdr.len, first hdr.len bytes)`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FidKey {
    len: u16,
    bytes: Vec<u8>,
}

impl FidKey {
    pub fn new(f: &[u8]) -> Self {
        let len = hdr_len(f);
        let bytes = (0..len as usize).map(|i| get(f, i)).collect();
        FidKey { len, bytes }
    }
}

/// A `struct fid` 20 bytes long identifying just a filesystem (fsid with a
/// null file handle); used to hash the mount fd of a filesystem.
pub fn fsid_key(f: &[u8]) -> Vec<u8> {
    let mut k = vec![0u8; FID_HDR];
    for (i, b) in k[4..12].iter_mut().enumerate() {
        *b = get(f, 4 + i);
    }
    set_info_type(&mut k, FAN_EVENT_INFO_TYPE_FID);
    set_hdr_len(&mut k, FID_HDR as u16);
    k
}

/// Copy the `struct file_handle` part of a fid into a suitably aligned
/// buffer for `open_by_handle_at()`.
pub fn aligned_handle(f: &[u8]) -> Vec<u32> {
    let n = 8 + handle_bytes(f) as usize;
    let mut h = vec![0u32; (n + 3) / 4 + 1];
    let hb: &mut [u8] =
        unsafe { std::slice::from_raw_parts_mut(h.as_mut_ptr().cast::<u8>(), h.len() * 4) };
    for (i, b) in hb.iter_mut().take(n).enumerate() {
        *b = get(f, HANDLE_OFF + i);
    }
    h
}
