//! Error types for inotifytools

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when using inotifytools
#[derive(Error, Debug)]
pub enum InotifyToolsError {
    #[error("Failed to initialize inotify: {0}")]
    InotifyInit(#[from] std::io::Error),
    
    #[error("Failed to add watch for {0}: {1}")]
    WatchAdd(PathBuf, std::io::Error),
    
    #[error("Failed to read directory {0}: {1}")]
    ReadDir(PathBuf, std::io::Error),
    
    #[error("Failed to read events: {0}")]
    ReadEvents(std::io::Error),
    
    #[error("Regular expression error: {0}")]
    Regex(#[from] regex::Error),
    
    #[error("Lock error occurred")]
    LockError,
    
    #[error("Invalid event string: {0}")]
    InvalidEventString(String),
    
    #[error("Watch descriptor {0} not found")]
    WatchDescriptorNotFound(i32),
    
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),
    
    #[error("Permission denied for path: {0}")]
    PermissionDenied(PathBuf),
    
    #[error("Invalid timeout value: {0}")]
    InvalidTimeout(i64),
}