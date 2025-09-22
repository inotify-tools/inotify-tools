//! Event handling for inotifytools

use std::path::PathBuf;
use std::time::SystemTime;
use inotify::{EventMask as InotifyEventMask, WatchMask};
use crate::error::InotifyToolsError;

/// Represents different types of filesystem events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventMask(u32);

impl EventMask {
    pub const ACCESS: EventMask = EventMask(0x00000001);
    pub const MODIFY: EventMask = EventMask(0x00000002);
    pub const ATTRIB: EventMask = EventMask(0x00000004);
    pub const CLOSE_WRITE: EventMask = EventMask(0x00000008);
    pub const CLOSE_NOWRITE: EventMask = EventMask(0x00000010);
    pub const CLOSE: EventMask = EventMask(Self::CLOSE_WRITE.0 | Self::CLOSE_NOWRITE.0);
    pub const OPEN: EventMask = EventMask(0x00000020);
    pub const MOVED_FROM: EventMask = EventMask(0x00000040);
    pub const MOVED_TO: EventMask = EventMask(0x00000080);
    pub const MOVE: EventMask = EventMask(Self::MOVED_FROM.0 | Self::MOVED_TO.0);
    pub const CREATE: EventMask = EventMask(0x00000100);
    pub const DELETE: EventMask = EventMask(0x00000200);
    pub const DELETE_SELF: EventMask = EventMask(0x00000400);
    pub const MOVE_SELF: EventMask = EventMask(0x00000800);
    pub const UNMOUNT: EventMask = EventMask(0x00002000);
    pub const Q_OVERFLOW: EventMask = EventMask(0x00004000);
    pub const IGNORED: EventMask = EventMask(0x00008000);
    pub const ISDIR: EventMask = EventMask(0x40000000);
    
    pub const ALL_EVENTS: EventMask = EventMask(
        Self::ACCESS.0 | Self::MODIFY.0 | Self::ATTRIB.0 |
        Self::CLOSE_WRITE.0 | Self::CLOSE_NOWRITE.0 | Self::OPEN.0 |
        Self::MOVED_FROM.0 | Self::MOVED_TO.0 | Self::CREATE.0 |
        Self::DELETE.0 | Self::DELETE_SELF.0 | Self::MOVE_SELF.0
    );

    /// Create a new EventMask from a raw value
    pub fn new(mask: u32) -> Self {
        EventMask(mask)
    }

    /// Get the raw mask value
    pub fn mask(&self) -> u32 {
        self.0
    }

    /// Check if this mask contains the given event
    pub fn contains(&self, other: EventMask) -> bool {
        (self.0 & other.0) != 0
    }

    /// Combine this mask with another
    pub fn union(&self, other: EventMask) -> EventMask {
        EventMask(self.0 | other.0)
    }

    /// Convert to inotify WatchMask
    pub fn to_watch_mask(&self) -> WatchMask {
        let mut mask = WatchMask::empty();
        
        if self.contains(Self::ACCESS) { mask |= WatchMask::ACCESS; }
        if self.contains(Self::MODIFY) { mask |= WatchMask::MODIFY; }
        if self.contains(Self::ATTRIB) { mask |= WatchMask::ATTRIB; }
        if self.contains(Self::CLOSE_WRITE) { mask |= WatchMask::CLOSE_WRITE; }
        if self.contains(Self::CLOSE_NOWRITE) { mask |= WatchMask::CLOSE_NOWRITE; }
        if self.contains(Self::OPEN) { mask |= WatchMask::OPEN; }
        if self.contains(Self::MOVED_FROM) { mask |= WatchMask::MOVED_FROM; }
        if self.contains(Self::MOVED_TO) { mask |= WatchMask::MOVED_TO; }
        if self.contains(Self::CREATE) { mask |= WatchMask::CREATE; }
        if self.contains(Self::DELETE) { mask |= WatchMask::DELETE; }
        if self.contains(Self::DELETE_SELF) { mask |= WatchMask::DELETE_SELF; }
        if self.contains(Self::MOVE_SELF) { mask |= WatchMask::MOVE_SELF; }
        
        mask
    }

    /// Convert from inotify EventMask
    pub fn from_inotify_event_mask(mask: InotifyEventMask) -> Self {
        let mut result = 0u32;
        
        if mask.contains(InotifyEventMask::ACCESS) { result |= Self::ACCESS.0; }
        if mask.contains(InotifyEventMask::MODIFY) { result |= Self::MODIFY.0; }
        if mask.contains(InotifyEventMask::ATTRIB) { result |= Self::ATTRIB.0; }
        if mask.contains(InotifyEventMask::CLOSE_WRITE) { result |= Self::CLOSE_WRITE.0; }
        if mask.contains(InotifyEventMask::CLOSE_NOWRITE) { result |= Self::CLOSE_NOWRITE.0; }
        if mask.contains(InotifyEventMask::OPEN) { result |= Self::OPEN.0; }
        if mask.contains(InotifyEventMask::MOVED_FROM) { result |= Self::MOVED_FROM.0; }
        if mask.contains(InotifyEventMask::MOVED_TO) { result |= Self::MOVED_TO.0; }
        if mask.contains(InotifyEventMask::CREATE) { result |= Self::CREATE.0; }
        if mask.contains(InotifyEventMask::DELETE) { result |= Self::DELETE.0; }
        if mask.contains(InotifyEventMask::DELETE_SELF) { result |= Self::DELETE_SELF.0; }
        if mask.contains(InotifyEventMask::MOVE_SELF) { result |= Self::MOVE_SELF.0; }
        if mask.contains(InotifyEventMask::ISDIR) { result |= Self::ISDIR.0; }
        
        EventMask(result)
    }
}

impl std::ops::BitOr for EventMask {
    type Output = Self;
    
    fn bitor(self, rhs: Self) -> Self::Output {
        EventMask(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for EventMask {
    type Output = Self;
    
    fn bitand(self, rhs: Self) -> Self::Output {
        EventMask(self.0 & rhs.0)
    }
}

/// Represents a filesystem event
#[derive(Debug, Clone)]
pub struct Event {
    pub wd: i32,
    pub mask: EventMask,
    pub cookie: u32,
    pub name: Option<String>,
    pub path: PathBuf,
    pub timestamp: SystemTime,
}

impl Event {
    /// Create a new event
    pub fn new(wd: i32, mask: EventMask, cookie: u32, name: Option<String>, path: PathBuf) -> Self {
        Self {
            wd,
            mask,
            cookie,
            name,
            path,
            timestamp: SystemTime::now(),
        }
    }

    /// Convert from inotify event
    pub fn from_inotify_event(
        event: inotify::Event<&std::ffi::OsStr>,
        watch_path: PathBuf,
    ) -> Result<Self, InotifyToolsError> {
        let wd = event.wd.get_watch_descriptor_id() as i32;
        let mask = EventMask::from_inotify_event_mask(event.mask);
        let cookie = event.cookie;
        
        let name = event.name.map(|n| n.to_string_lossy().to_string());
        
        // Construct full path
        let path = if let Some(ref name) = name {
            watch_path.join(name)
        } else {
            watch_path
        };
        
        Ok(Event::new(wd, mask, cookie, name, path))
    }

    /// Check if this is a directory event
    pub fn is_dir(&self) -> bool {
        self.mask.contains(EventMask::ISDIR)
    }

    /// Get event type as string
    pub fn event_type_str(&self) -> Vec<&'static str> {
        let mut types = Vec::new();
        
        if self.mask.contains(EventMask::ACCESS) { types.push("ACCESS"); }
        if self.mask.contains(EventMask::MODIFY) { types.push("MODIFY"); }
        if self.mask.contains(EventMask::ATTRIB) { types.push("ATTRIB"); }
        if self.mask.contains(EventMask::CLOSE_WRITE) { types.push("CLOSE_WRITE"); }
        if self.mask.contains(EventMask::CLOSE_NOWRITE) { types.push("CLOSE_NOWRITE"); }
        if self.mask.contains(EventMask::OPEN) { types.push("OPEN"); }
        if self.mask.contains(EventMask::MOVED_FROM) { types.push("MOVED_FROM"); }
        if self.mask.contains(EventMask::MOVED_TO) { types.push("MOVED_TO"); }
        if self.mask.contains(EventMask::CREATE) { types.push("CREATE"); }
        if self.mask.contains(EventMask::DELETE) { types.push("DELETE"); }
        if self.mask.contains(EventMask::DELETE_SELF) { types.push("DELETE_SELF"); }
        if self.mask.contains(EventMask::MOVE_SELF) { types.push("MOVE_SELF"); }
        if self.mask.contains(EventMask::UNMOUNT) { types.push("UNMOUNT"); }
        
        if types.is_empty() {
            types.push("UNKNOWN");
        }
        
        types
    }
}

/// Convert string representation of events to EventMask
pub fn str_to_events(event_str: &str) -> Result<EventMask, InotifyToolsError> {
    str_to_events_sep(event_str, ',')
}

/// Convert string representation of events to EventMask with custom separator
pub fn str_to_events_sep(event_str: &str, sep: char) -> Result<EventMask, InotifyToolsError> {
    if event_str.is_empty() {
        return Ok(EventMask::new(0));
    }
    
    let mut result = EventMask::new(0);
    
    for part in event_str.split(sep) {
        let part = part.trim().to_uppercase();
        
        let event_mask = match part.as_str() {
            "ACCESS" => EventMask::ACCESS,
            "MODIFY" => EventMask::MODIFY,
            "ATTRIB" => EventMask::ATTRIB,
            "CLOSE_WRITE" => EventMask::CLOSE_WRITE,
            "CLOSE_NOWRITE" => EventMask::CLOSE_NOWRITE,
            "CLOSE" => EventMask::CLOSE,
            "OPEN" => EventMask::OPEN,
            "MOVED_FROM" => EventMask::MOVED_FROM,
            "MOVED_TO" => EventMask::MOVED_TO,
            "MOVE" => EventMask::MOVE,
            "CREATE" => EventMask::CREATE,
            "DELETE" => EventMask::DELETE,
            "DELETE_SELF" => EventMask::DELETE_SELF,
            "MOVE_SELF" => EventMask::MOVE_SELF,
            "UNMOUNT" => EventMask::UNMOUNT,
            "ALL_EVENTS" => EventMask::ALL_EVENTS,
            _ => return Err(InotifyToolsError::InvalidEventString(part)),
        };
        
        result = result.union(event_mask);
    }
    
    Ok(result)
}

/// Convert EventMask to string representation
pub fn events_to_str(events: EventMask) -> String {
    events_to_str_sep(events, ',')
}

/// Convert EventMask to string representation with custom separator
pub fn events_to_str_sep(events: EventMask, sep: char) -> String {
    let mut parts = Vec::new();
    
    if events.contains(EventMask::ACCESS) { parts.push("ACCESS"); }
    if events.contains(EventMask::MODIFY) { parts.push("MODIFY"); }
    if events.contains(EventMask::ATTRIB) { parts.push("ATTRIB"); }
    if events.contains(EventMask::CLOSE_WRITE) { parts.push("CLOSE_WRITE"); }
    if events.contains(EventMask::CLOSE_NOWRITE) { parts.push("CLOSE_NOWRITE"); }
    if events.contains(EventMask::OPEN) { parts.push("OPEN"); }
    if events.contains(EventMask::MOVED_FROM) { parts.push("MOVED_FROM"); }
    if events.contains(EventMask::MOVED_TO) { parts.push("MOVED_TO"); }
    if events.contains(EventMask::CREATE) { parts.push("CREATE"); }
    if events.contains(EventMask::DELETE) { parts.push("DELETE"); }
    if events.contains(EventMask::DELETE_SELF) { parts.push("DELETE_SELF"); }
    if events.contains(EventMask::MOVE_SELF) { parts.push("MOVE_SELF"); }
    if events.contains(EventMask::UNMOUNT) { parts.push("UNMOUNT"); }
    
    if parts.is_empty() {
        "NONE".to_string()
    } else {
        parts.join(&sep.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_mask_operations() {
        let mask1 = EventMask::MODIFY;
        let mask2 = EventMask::CREATE;
        let combined = mask1 | mask2;
        
        assert!(combined.contains(EventMask::MODIFY));
        assert!(combined.contains(EventMask::CREATE));
        assert!(!combined.contains(EventMask::DELETE));
    }
    
    #[test]
    fn test_str_to_events() {
        let result = str_to_events("modify,create,delete").unwrap();
        assert!(result.contains(EventMask::MODIFY));
        assert!(result.contains(EventMask::CREATE));
        assert!(result.contains(EventMask::DELETE));
    }
    
    #[test]
    fn test_events_to_str() {
        let events = EventMask::MODIFY | EventMask::CREATE;
        let result = events_to_str(events);
        assert!(result.contains("MODIFY"));
        assert!(result.contains("CREATE"));
    }
}