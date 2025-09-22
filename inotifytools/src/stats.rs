//! Statistics collection for inotifytools

use std::collections::HashMap;
use std::path::PathBuf;
use crate::event::{Event, EventMask};

/// Statistics for filesystem events
#[derive(Debug, Clone)]
pub struct Statistics {
    /// Total events by type
    pub total_access: u64,
    pub total_modify: u64,
    pub total_attrib: u64,
    pub total_close_write: u64,
    pub total_close_nowrite: u64,
    pub total_open: u64,
    pub total_moved_from: u64,
    pub total_moved_to: u64,
    pub total_create: u64,
    pub total_delete: u64,
    pub total_delete_self: u64,
    pub total_unmount: u64,
    pub total_move_self: u64,
    pub total_events: u64,
    
    /// Per-file statistics
    pub file_stats: HashMap<PathBuf, FileStatistics>,
}

/// Statistics for a specific file or directory
#[derive(Debug, Clone)]
pub struct FileStatistics {
    pub path: PathBuf,
    pub access: u64,
    pub modify: u64,
    pub attrib: u64,
    pub close_write: u64,
    pub close_nowrite: u64,
    pub open: u64,
    pub moved_from: u64,
    pub moved_to: u64,
    pub create: u64,
    pub delete: u64,
    pub delete_self: u64,
    pub unmount: u64,
    pub move_self: u64,
    pub total: u64,
}

impl Statistics {
    /// Create new statistics instance
    pub fn new() -> Self {
        Self {
            total_access: 0,
            total_modify: 0,
            total_attrib: 0,
            total_close_write: 0,
            total_close_nowrite: 0,
            total_open: 0,
            total_moved_from: 0,
            total_moved_to: 0,
            total_create: 0,
            total_delete: 0,
            total_delete_self: 0,
            total_unmount: 0,
            total_move_self: 0,
            total_events: 0,
            file_stats: HashMap::new(),
        }
    }

    /// Record an event in statistics
    pub fn record_event(&mut self, event: &Event) {
        self.update_totals(event.mask);
        self.update_file_stats(&event.path, event.mask);
    }

    /// Update total statistics
    fn update_totals(&mut self, mask: EventMask) {
        if mask.contains(EventMask::ACCESS) {
            self.total_access += 1;
        }
        if mask.contains(EventMask::MODIFY) {
            self.total_modify += 1;
        }
        if mask.contains(EventMask::ATTRIB) {
            self.total_attrib += 1;
        }
        if mask.contains(EventMask::CLOSE_WRITE) {
            self.total_close_write += 1;
        }
        if mask.contains(EventMask::CLOSE_NOWRITE) {
            self.total_close_nowrite += 1;
        }
        if mask.contains(EventMask::OPEN) {
            self.total_open += 1;
        }
        if mask.contains(EventMask::MOVED_FROM) {
            self.total_moved_from += 1;
        }
        if mask.contains(EventMask::MOVED_TO) {
            self.total_moved_to += 1;
        }
        if mask.contains(EventMask::CREATE) {
            self.total_create += 1;
        }
        if mask.contains(EventMask::DELETE) {
            self.total_delete += 1;
        }
        if mask.contains(EventMask::DELETE_SELF) {
            self.total_delete_self += 1;
        }
        if mask.contains(EventMask::UNMOUNT) {
            self.total_unmount += 1;
        }
        if mask.contains(EventMask::MOVE_SELF) {
            self.total_move_self += 1;
        }
        
        self.total_events += 1;
    }

    /// Update per-file statistics
    fn update_file_stats(&mut self, path: &PathBuf, mask: EventMask) {
        let stats = self.file_stats.entry(path.clone()).or_insert_with(|| {
            FileStatistics::new(path.clone())
        });
        
        stats.record_event(mask);
    }

    /// Get total count for specific event type
    pub fn get_total_count(&self, mask: EventMask) -> u64 {
        let mut count = 0;
        
        if mask.contains(EventMask::ACCESS) {
            count += self.total_access;
        }
        if mask.contains(EventMask::MODIFY) {
            count += self.total_modify;
        }
        if mask.contains(EventMask::ATTRIB) {
            count += self.total_attrib;
        }
        if mask.contains(EventMask::CLOSE_WRITE) {
            count += self.total_close_write;
        }
        if mask.contains(EventMask::CLOSE_NOWRITE) {
            count += self.total_close_nowrite;
        }
        if mask.contains(EventMask::OPEN) {
            count += self.total_open;
        }
        if mask.contains(EventMask::MOVED_FROM) {
            count += self.total_moved_from;
        }
        if mask.contains(EventMask::MOVED_TO) {
            count += self.total_moved_to;
        }
        if mask.contains(EventMask::CREATE) {
            count += self.total_create;
        }
        if mask.contains(EventMask::DELETE) {
            count += self.total_delete;
        }
        if mask.contains(EventMask::DELETE_SELF) {
            count += self.total_delete_self;
        }
        if mask.contains(EventMask::UNMOUNT) {
            count += self.total_unmount;
        }
        if mask.contains(EventMask::MOVE_SELF) {
            count += self.total_move_self;
        }
        
        count
    }

    /// Get statistics for a specific file
    pub fn get_file_stats(&self, path: &PathBuf) -> Option<&FileStatistics> {
        self.file_stats.get(path)
    }

    /// Get all file statistics sorted by total events
    pub fn get_sorted_file_stats(&self, ascending: bool) -> Vec<&FileStatistics> {
        let mut stats: Vec<&FileStatistics> = self.file_stats.values().collect();
        
        if ascending {
            stats.sort_by(|a, b| a.total.cmp(&b.total));
        } else {
            stats.sort_by(|a, b| b.total.cmp(&a.total));
        }
        
        stats
    }

    /// Get file statistics sorted by specific event type
    pub fn get_sorted_file_stats_by_event(&self, mask: EventMask, ascending: bool) -> Vec<&FileStatistics> {
        let mut stats: Vec<&FileStatistics> = self.file_stats.values().collect();
        
        if ascending {
            stats.sort_by(|a, b| a.get_count(mask).cmp(&b.get_count(mask)));
        } else {
            stats.sort_by(|a, b| b.get_count(mask).cmp(&a.get_count(mask)));
        }
        
        stats
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        self.total_access = 0;
        self.total_modify = 0;
        self.total_attrib = 0;
        self.total_close_write = 0;
        self.total_close_nowrite = 0;
        self.total_open = 0;
        self.total_moved_from = 0;
        self.total_moved_to = 0;
        self.total_create = 0;
        self.total_delete = 0;
        self.total_delete_self = 0;
        self.total_unmount = 0;
        self.total_move_self = 0;
        self.total_events = 0;
        self.file_stats.clear();
    }
}

impl Default for Statistics {
    fn default() -> Self {
        Self::new()
    }
}

impl FileStatistics {
    /// Create new file statistics
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            access: 0,
            modify: 0,
            attrib: 0,
            close_write: 0,
            close_nowrite: 0,
            open: 0,
            moved_from: 0,
            moved_to: 0,
            create: 0,
            delete: 0,
            delete_self: 0,
            unmount: 0,
            move_self: 0,
            total: 0,
        }
    }

    /// Record an event
    pub fn record_event(&mut self, mask: EventMask) {
        if mask.contains(EventMask::ACCESS) {
            self.access += 1;
        }
        if mask.contains(EventMask::MODIFY) {
            self.modify += 1;
        }
        if mask.contains(EventMask::ATTRIB) {
            self.attrib += 1;
        }
        if mask.contains(EventMask::CLOSE_WRITE) {
            self.close_write += 1;
        }
        if mask.contains(EventMask::CLOSE_NOWRITE) {
            self.close_nowrite += 1;
        }
        if mask.contains(EventMask::OPEN) {
            self.open += 1;
        }
        if mask.contains(EventMask::MOVED_FROM) {
            self.moved_from += 1;
        }
        if mask.contains(EventMask::MOVED_TO) {
            self.moved_to += 1;
        }
        if mask.contains(EventMask::CREATE) {
            self.create += 1;
        }
        if mask.contains(EventMask::DELETE) {
            self.delete += 1;
        }
        if mask.contains(EventMask::DELETE_SELF) {
            self.delete_self += 1;
        }
        if mask.contains(EventMask::UNMOUNT) {
            self.unmount += 1;
        }
        if mask.contains(EventMask::MOVE_SELF) {
            self.move_self += 1;
        }
        
        self.total += 1;
    }

    /// Get count for specific event type
    pub fn get_count(&self, mask: EventMask) -> u64 {
        let mut count = 0;
        
        if mask.contains(EventMask::ACCESS) {
            count += self.access;
        }
        if mask.contains(EventMask::MODIFY) {
            count += self.modify;
        }
        if mask.contains(EventMask::ATTRIB) {
            count += self.attrib;
        }
        if mask.contains(EventMask::CLOSE_WRITE) {
            count += self.close_write;
        }
        if mask.contains(EventMask::CLOSE_NOWRITE) {
            count += self.close_nowrite;
        }
        if mask.contains(EventMask::OPEN) {
            count += self.open;
        }
        if mask.contains(EventMask::MOVED_FROM) {
            count += self.moved_from;
        }
        if mask.contains(EventMask::MOVED_TO) {
            count += self.moved_to;
        }
        if mask.contains(EventMask::CREATE) {
            count += self.create;
        }
        if mask.contains(EventMask::DELETE) {
            count += self.delete;
        }
        if mask.contains(EventMask::DELETE_SELF) {
            count += self.delete_self;
        }
        if mask.contains(EventMask::UNMOUNT) {
            count += self.unmount;
        }
        if mask.contains(EventMask::MOVE_SELF) {
            count += self.move_self;
        }
        
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_statistics_creation() {
        let stats = Statistics::new();
        assert_eq!(stats.total_events, 0);
        assert!(stats.file_stats.is_empty());
    }
    
    #[test]
    fn test_event_recording() {
        let mut stats = Statistics::new();
        let event = Event::new(
            1,
            EventMask::MODIFY,
            0,
            None,
            PathBuf::from("/tmp/test.txt")
        );
        
        stats.record_event(&event);
        
        assert_eq!(stats.total_modify, 1);
        assert_eq!(stats.total_events, 1);
        assert_eq!(stats.file_stats.len(), 1);
    }
    
    #[test]
    fn test_file_statistics() {
        let mut file_stats = FileStatistics::new(PathBuf::from("/tmp/test.txt"));
        
        file_stats.record_event(EventMask::MODIFY);
        assert_eq!(file_stats.modify, 1);
        assert_eq!(file_stats.total, 1);
        
        file_stats.record_event(EventMask::CREATE | EventMask::MODIFY);
        assert_eq!(file_stats.modify, 2);
        assert_eq!(file_stats.create, 1);
        assert_eq!(file_stats.total, 2);
    }
}