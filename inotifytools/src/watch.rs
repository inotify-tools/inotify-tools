//! Watch management for inotifytools

use std::path::PathBuf;

/// Represents a filesystem watch
#[derive(Debug, Clone)]
pub struct Watch {
    pub wd: i32,
    pub path: PathBuf,
    pub is_directory: bool,
    pub hit_access: u64,
    pub hit_modify: u64,
    pub hit_attrib: u64,
    pub hit_close_write: u64,
    pub hit_close_nowrite: u64,
    pub hit_open: u64,
    pub hit_moved_from: u64,
    pub hit_moved_to: u64,
    pub hit_create: u64,
    pub hit_delete: u64,
    pub hit_delete_self: u64,
    pub hit_unmount: u64,
    pub hit_move_self: u64,
    pub hit_total: u64,
}

impl Watch {
    /// Create a new watch
    pub fn new(wd: i32, path: PathBuf) -> Self {
        let is_directory = path.is_dir();
        
        Self {
            wd,
            path,
            is_directory,
            hit_access: 0,
            hit_modify: 0,
            hit_attrib: 0,
            hit_close_write: 0,
            hit_close_nowrite: 0,
            hit_open: 0,
            hit_moved_from: 0,
            hit_moved_to: 0,
            hit_create: 0,
            hit_delete: 0,
            hit_delete_self: 0,
            hit_unmount: 0,
            hit_move_self: 0,
            hit_total: 0,
        }
    }

    /// Record an event hit
    pub fn record_event(&mut self, event_mask: crate::event::EventMask) {
        use crate::event::EventMask;
        
        if event_mask.contains(EventMask::ACCESS) {
            self.hit_access += 1;
        }
        if event_mask.contains(EventMask::MODIFY) {
            self.hit_modify += 1;
        }
        if event_mask.contains(EventMask::ATTRIB) {
            self.hit_attrib += 1;
        }
        if event_mask.contains(EventMask::CLOSE_WRITE) {
            self.hit_close_write += 1;
        }
        if event_mask.contains(EventMask::CLOSE_NOWRITE) {
            self.hit_close_nowrite += 1;
        }
        if event_mask.contains(EventMask::OPEN) {
            self.hit_open += 1;
        }
        if event_mask.contains(EventMask::MOVED_FROM) {
            self.hit_moved_from += 1;
        }
        if event_mask.contains(EventMask::MOVED_TO) {
            self.hit_moved_to += 1;
        }
        if event_mask.contains(EventMask::CREATE) {
            self.hit_create += 1;
        }
        if event_mask.contains(EventMask::DELETE) {
            self.hit_delete += 1;
        }
        if event_mask.contains(EventMask::DELETE_SELF) {
            self.hit_delete_self += 1;
        }
        if event_mask.contains(EventMask::UNMOUNT) {
            self.hit_unmount += 1;
        }
        if event_mask.contains(EventMask::MOVE_SELF) {
            self.hit_move_self += 1;
        }
        
        self.hit_total += 1;
    }

    /// Get hit count for specific event type
    pub fn get_hit_count(&self, event_mask: crate::event::EventMask) -> u64 {
        use crate::event::EventMask;
        
        let mut count = 0;
        
        if event_mask.contains(EventMask::ACCESS) {
            count += self.hit_access;
        }
        if event_mask.contains(EventMask::MODIFY) {
            count += self.hit_modify;
        }
        if event_mask.contains(EventMask::ATTRIB) {
            count += self.hit_attrib;
        }
        if event_mask.contains(EventMask::CLOSE_WRITE) {
            count += self.hit_close_write;
        }
        if event_mask.contains(EventMask::CLOSE_NOWRITE) {
            count += self.hit_close_nowrite;
        }
        if event_mask.contains(EventMask::OPEN) {
            count += self.hit_open;
        }
        if event_mask.contains(EventMask::MOVED_FROM) {
            count += self.hit_moved_from;
        }
        if event_mask.contains(EventMask::MOVED_TO) {
            count += self.hit_moved_to;
        }
        if event_mask.contains(EventMask::CREATE) {
            count += self.hit_create;
        }
        if event_mask.contains(EventMask::DELETE) {
            count += self.hit_delete;
        }
        if event_mask.contains(EventMask::DELETE_SELF) {
            count += self.hit_delete_self;
        }
        if event_mask.contains(EventMask::UNMOUNT) {
            count += self.hit_unmount;
        }
        if event_mask.contains(EventMask::MOVE_SELF) {
            count += self.hit_move_self;
        }
        
        count
    }

    /// Reset all statistics
    pub fn reset_stats(&mut self) {
        self.hit_access = 0;
        self.hit_modify = 0;
        self.hit_attrib = 0;
        self.hit_close_write = 0;
        self.hit_close_nowrite = 0;
        self.hit_open = 0;
        self.hit_moved_from = 0;
        self.hit_moved_to = 0;
        self.hit_create = 0;
        self.hit_delete = 0;
        self.hit_delete_self = 0;
        self.hit_unmount = 0;
        self.hit_move_self = 0;
        self.hit_total = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventMask;
    use std::path::Path;
    
    #[test]
    fn test_watch_creation() {
        let path = PathBuf::from("/tmp/test");
        let watch = Watch::new(1, path.clone());
        
        assert_eq!(watch.wd, 1);
        assert_eq!(watch.path, path);
        assert_eq!(watch.hit_total, 0);
    }
    
    #[test]
    fn test_event_recording() {
        let path = PathBuf::from("/tmp/test");
        let mut watch = Watch::new(1, path);
        
        watch.record_event(EventMask::MODIFY);
        assert_eq!(watch.hit_modify, 1);
        assert_eq!(watch.hit_total, 1);
        
        watch.record_event(EventMask::CREATE | EventMask::MODIFY);
        assert_eq!(watch.hit_modify, 2);
        assert_eq!(watch.hit_create, 1);
        assert_eq!(watch.hit_total, 2);
    }
    
    #[test]
    fn test_hit_count_retrieval() {
        let path = PathBuf::from("/tmp/test");
        let mut watch = Watch::new(1, path);
        
        watch.record_event(EventMask::MODIFY);
        watch.record_event(EventMask::CREATE);
        
        assert_eq!(watch.get_hit_count(EventMask::MODIFY), 1);
        assert_eq!(watch.get_hit_count(EventMask::CREATE), 1);
        assert_eq!(watch.get_hit_count(EventMask::MODIFY | EventMask::CREATE), 2);
    }
}