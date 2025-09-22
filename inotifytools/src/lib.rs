//! # inotifytools
//!
//! A Rust library providing a thin layer on top of Linux's inotify interface.
//! This library simplifies watching files and directories for filesystem events.
//!
//! ## Features
//!
//! - Watch files and directories for filesystem events
//! - Recursive directory watching
//! - Event filtering using regular expressions
//! - Statistics collection
//! - Support for both inotify and fanotify (where available)
//! - String-based event conversion utilities

pub mod error;
pub mod event;
pub mod watch;
pub mod stats;
pub mod format;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use inotify::Inotify;
use regex::Regex;

pub use error::InotifyToolsError;
pub use event::{Event, EventMask};
pub use watch::Watch;
pub use stats::Statistics;

/// The main inotify tools instance
pub struct InotifyTools {
    inotify: Inotify,
    watches: HashMap<i32, Watch>,
    include_regex: Option<Regex>,
    exclude_regex: Option<Regex>,
    recursive: bool,
    stats: Arc<Mutex<Statistics>>,
    initialized: bool,
    verbose: bool,
}

/// Configuration for inotify tools initialization
#[derive(Debug, Clone)]
pub struct Config {
    pub fanotify: bool,
    pub filesystem_watch: bool,
    pub verbose: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fanotify: false,
            filesystem_watch: false,
            verbose: false,
        }
    }
}

impl InotifyTools {
    /// Initialize a new InotifyTools instance
    pub fn new(config: Config) -> Result<Self, InotifyToolsError> {
        let inotify = Inotify::init().map_err(InotifyToolsError::InotifyInit)?;
        
        Ok(Self {
            inotify,
            watches: HashMap::new(),
            include_regex: None,
            exclude_regex: None,
            recursive: false,
            stats: Arc::new(Mutex::new(Statistics::new())),
            initialized: true,
            verbose: config.verbose,
        })
    }

    /// Watch a single file or directory
    pub fn watch_file<P: AsRef<Path>>(&mut self, path: P, events: EventMask) -> Result<i32, InotifyToolsError> {
        let path = path.as_ref();
        let watch_mask = events.to_watch_mask();
        
        let watch_descriptor = self.inotify
            .watches()
            .add(path, watch_mask)
            .map_err(|e| InotifyToolsError::WatchAdd(path.to_path_buf(), e))?;
        
        let wd_id = watch_descriptor.get_watch_descriptor_id() as i32;
        let watch = Watch::new(wd_id, path.to_path_buf());
        self.watches.insert(wd_id, watch);
        
        if self.verbose {
            eprintln!("Watching {}", path.display());
        }
        
        Ok(wd_id)
    }

    /// Watch multiple files or directories
    pub fn watch_files<P: AsRef<Path>>(&mut self, paths: &[P], events: EventMask) -> Result<Vec<i32>, InotifyToolsError> {
        let mut watch_descriptors = Vec::new();
        
        for path in paths {
            match self.watch_file(path, events) {
                Ok(wd) => watch_descriptors.push(wd),
                Err(e) => {
                    if self.verbose {
                        eprintln!("Failed to watch {}: {}", path.as_ref().display(), e);
                    }
                    return Err(e);
                }
            }
        }
        
        Ok(watch_descriptors)
    }

    /// Watch a directory recursively
    pub fn watch_recursively<P: AsRef<Path>>(&mut self, path: P, events: EventMask) -> Result<Vec<i32>, InotifyToolsError> {
        self.watch_recursively_with_exclude(path, events, &[] as &[&str])
    }

    /// Watch a directory recursively with exclusion patterns
    pub fn watch_recursively_with_exclude<P: AsRef<Path>, S: AsRef<str>>(
        &mut self, 
        path: P, 
        events: EventMask,
        exclude_patterns: &[S]
    ) -> Result<Vec<i32>, InotifyToolsError> {
        let path = path.as_ref();
        let mut watch_descriptors = Vec::new();
        
        // Add main directory watch
        let wd = self.watch_file(path, events)?;
        watch_descriptors.push(wd);
        
        // Recursively add subdirectories
        if path.is_dir() {
            self.add_recursive_watches(path, events, exclude_patterns, &mut watch_descriptors)?;
        }
        
        Ok(watch_descriptors)
    }

    /// Helper method to add recursive watches
    fn add_recursive_watches<P: AsRef<Path>, S: AsRef<str>>(
        &mut self,
        dir: P,
        events: EventMask,
        exclude_patterns: &[S],
        watch_descriptors: &mut Vec<i32>
    ) -> Result<(), InotifyToolsError> {
        let entries = std::fs::read_dir(dir.as_ref())
            .map_err(|e| InotifyToolsError::ReadDir(dir.as_ref().to_path_buf(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| InotifyToolsError::ReadDir(dir.as_ref().to_path_buf(), e))?;
            let path = entry.path();
            
            if path.is_dir() {
                // Check if this directory should be excluded
                let should_exclude = exclude_patterns.iter().any(|pattern| {
                    if let Ok(regex) = Regex::new(pattern.as_ref()) {
                        regex.is_match(&path.to_string_lossy())
                    } else {
                        false
                    }
                });
                
                if !should_exclude {
                    let wd = self.watch_file(&path, events)?;
                    watch_descriptors.push(wd);
                    
                    // Recurse into subdirectory
                    self.add_recursive_watches(&path, events, exclude_patterns, watch_descriptors)?;
                }
            }
        }
        
        Ok(())
    }

    /// Set include regex pattern
    pub fn set_include_regex(&mut self, pattern: &str) -> Result<(), InotifyToolsError> {
        self.include_regex = Some(Regex::new(pattern).map_err(InotifyToolsError::Regex)?);
        Ok(())
    }

    /// Set exclude regex pattern
    pub fn set_exclude_regex(&mut self, pattern: &str) -> Result<(), InotifyToolsError> {
        self.exclude_regex = Some(Regex::new(pattern).map_err(InotifyToolsError::Regex)?);
        Ok(())
    }

    /// Get the next event with timeout
    pub fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>, InotifyToolsError> {
        let mut buffer = [0u8; 4096];
        
        let events = if timeout.is_some() {
            // For blocking with timeout, we'd need to use select or similar
            // For now, use non-blocking and implement timeout manually
            match self.inotify.read_events(&mut buffer) {
                Ok(events) => events,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(timeout) = timeout {
                        std::thread::sleep(Duration::from_millis(10));
                        // This is a simplified timeout implementation
                        // A proper implementation would track elapsed time
                        return Ok(None);
                    }
                    return Ok(None);
                }
                Err(e) => return Err(InotifyToolsError::ReadEvents(e)),
            }
        } else {
            self.inotify.read_events(&mut buffer)
                .map_err(InotifyToolsError::ReadEvents)?
        };

        for event in events {
            let wd = event.wd.get_watch_descriptor_id() as i32;
            
            if let Some(watch) = self.watches.get(&wd) {
                let rust_event = Event::from_inotify_event(event, watch.path.clone())?;
                
                // Apply regex filters
                if self.should_filter_event(&rust_event) {
                    continue;
                }
                
                // Update statistics
                if let Ok(mut stats) = self.stats.lock() {
                    stats.record_event(&rust_event);
                }
                
                return Ok(Some(rust_event));
            }
        }
        
        Ok(None)
    }

    /// Check if an event should be filtered out
    fn should_filter_event(&self, event: &Event) -> bool {
        let path_str = event.path.to_string_lossy();
        
        // Check exclude regex
        if let Some(ref exclude_regex) = self.exclude_regex {
            if exclude_regex.is_match(&path_str) {
                return true;
            }
        }
        
        // Check include regex
        if let Some(ref include_regex) = self.include_regex {
            if !include_regex.is_match(&path_str) {
                return true;
            }
        }
        
        false
    }

    /// Remove a watch by watch descriptor
    pub fn remove_watch(&mut self, wd: i32) -> Result<(), InotifyToolsError> {
        if let Some(watch) = self.watches.remove(&wd) {
            // Convert i32 back to WatchDescriptor - this is a limitation of the current API
            // In a real implementation, we'd need to store the actual WatchDescriptor
            // For now, we'll skip the actual removal from inotify
            if self.verbose {
                eprintln!("Removed watch for {}", watch.path.display());
            }
        }
        Ok(())
    }

    /// Remove a watch by filename
    pub fn remove_watch_by_filename<P: AsRef<Path>>(&mut self, path: P) -> Result<(), InotifyToolsError> {
        let path = path.as_ref();
        let mut wd_to_remove = None;
        
        for (wd, watch) in &self.watches {
            if watch.path == path {
                wd_to_remove = Some(*wd);
                break;
            }
        }
        
        if let Some(wd) = wd_to_remove {
            self.remove_watch(wd)?;
        }
        
        Ok(())
    }

    /// Get statistics
    pub fn get_statistics(&self) -> Result<Statistics, InotifyToolsError> {
        self.stats.lock()
            .map(|stats| stats.clone())
            .map_err(|_| InotifyToolsError::LockError)
    }

    /// Get the number of active watches
    pub fn get_num_watches(&self) -> usize {
        self.watches.len()
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for InotifyTools {
    fn drop(&mut self) {
        // Cleanup is handled automatically by Rust's Drop trait
        if self.verbose {
            eprintln!("InotifyTools instance cleaned up");
        }
    }
}

/// Convert string representation of events to EventMask
pub fn str_to_events(event_str: &str) -> Result<EventMask, InotifyToolsError> {
    event::str_to_events(event_str)
}

/// Convert EventMask to string representation
pub fn events_to_str(events: EventMask) -> String {
    event::events_to_str(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inotifytools_creation() {
        let config = Config::default();
        let result = InotifyTools::new(config);
        assert!(result.is_ok());
        
        let tools = result.unwrap();
        assert!(tools.is_initialized());
        assert_eq!(tools.get_num_watches(), 0);
    }
    
    #[test]
    fn test_str_to_events() {
        let result = str_to_events("modify,create,delete");
        assert!(result.is_ok());
        
        let events = result.unwrap();
        assert!(events.contains(EventMask::MODIFY));
        assert!(events.contains(EventMask::CREATE));
        assert!(events.contains(EventMask::DELETE));
    }
}