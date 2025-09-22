//! Output formatting utilities for inotifytools

use std::fmt::Write;
use std::path::Path;
use chrono::{DateTime, Local};
use crate::event::Event;
use crate::error::InotifyToolsError;

/// Formatter for inotify events
pub struct EventFormatter {
    time_format: Option<String>,
}

impl EventFormatter {
    /// Create a new event formatter
    pub fn new() -> Self {
        Self {
            time_format: None,
        }
    }

    /// Set time format string (strftime compatible)
    pub fn set_time_format(&mut self, format: String) {
        self.time_format = Some(format);
    }

    /// Clear time format
    pub fn clear_time_format(&mut self) {
        self.time_format = None;
    }

    /// Format an event using a format string
    pub fn format_event(&self, event: &Event, format_str: &str) -> Result<String, InotifyToolsError> {
        let mut result = String::new();
        let mut chars = format_str.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '%' {
                if let Some(&next_ch) = chars.peek() {
                    chars.next(); // consume the format character
                    match next_ch {
                        'w' => {
                            // Watch descriptor
                            write!(result, "{}", event.wd).unwrap();
                        }
                        'f' => {
                            // Filename
                            if let Some(ref name) = event.name {
                                result.push_str(name);
                            } else {
                                result.push_str(&event.path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy());
                            }
                        }
                        'F' => {
                            // Full path
                            result.push_str(&event.path.to_string_lossy());
                        }
                        'e' => {
                            // Event types (comma-separated)
                            let event_types = event.event_type_str();
                            result.push_str(&event_types.join(","));
                        }
                        _ if next_ch == 'X' => {
                            // Check for extended format like %Xe
                            if let Some(&'e') = chars.peek() {
                                chars.next(); // consume 'e'
                                // Event types (single)
                                let event_types = event.event_type_str();
                                if let Some(first) = event_types.first() {
                                    result.push_str(first);
                                }
                            } else {
                                // Unknown format, include as is
                                result.push('%');
                                result.push(next_ch);
                            }
                        }
                        'c' => {
                            // Cookie
                            write!(result, "{}", event.cookie).unwrap();
                        }
                        'T' => {
                            // Timestamp
                            if let Some(ref time_fmt) = self.time_format {
                                let datetime: DateTime<Local> = event.timestamp.into();
                                result.push_str(&datetime.format(time_fmt).to_string());
                            } else {
                                let datetime: DateTime<Local> = event.timestamp.into();
                                result.push_str(&datetime.format("%Y-%m-%d %H:%M:%S").to_string());
                            }
                        }
                        '%' => {
                            // Literal %
                            result.push('%');
                        }
                        _ => {
                            // Unknown format specifier, just include it
                            result.push('%');
                            result.push(next_ch);
                        }
                    }
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }
        
        Ok(result)
    }

    /// Format event in CSV format
    pub fn format_csv(&self, event: &Event) -> String {
        let watch_path = event.path.parent()
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();
        
        let filename = event.name.as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| {
                event.path.file_name()
                    .map(|name| name.to_str().unwrap_or(""))
                    .unwrap_or("")
            });

        let event_types = event.event_type_str().join(",");
        
        format!("{},{},{}", watch_path, filename, event_types)
    }

    /// Format event in default format
    pub fn format_default(&self, event: &Event) -> String {
        let watch_path = event.path.parent()
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();
        
        let filename = event.name.as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| {
                event.path.file_name()
                    .map(|name| name.to_str().unwrap_or(""))
                    .unwrap_or("")
            });

        let event_types = event.event_type_str().join(",");
        
        format!("{} {} {}", watch_path, event_types, filename)
    }
}

impl Default for EventFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// CSV formatter for statistics output
pub struct CsvFormatter;

impl CsvFormatter {
    /// Format file statistics as CSV
    pub fn format_file_stats<P: AsRef<Path>>(path: P, stats: &crate::stats::FileStatistics) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            path.as_ref().to_string_lossy(),
            stats.access,
            stats.modify,
            stats.attrib,
            stats.close_write,
            stats.close_nowrite,
            stats.open,
            stats.moved_from,
            stats.moved_to,
            stats.create,
            stats.delete,
            stats.delete_self,
            stats.move_self,
            stats.unmount,
            stats.total
        )
    }

    /// Get CSV header for file statistics
    pub fn file_stats_header() -> &'static str {
        "filename,access,modify,attrib,close_write,close_nowrite,open,moved_from,moved_to,create,delete,delete_self,move_self,unmount,total"
    }
}

/// Table formatter for statistics output
pub struct TableFormatter;

impl TableFormatter {
    /// Format statistics as a table
    pub fn format_stats_table(
        stats: &crate::stats::Statistics,
        sort_by: Option<crate::event::EventMask>,
        ascending: bool,
        show_zeros: bool
    ) -> String {
        let mut output = String::new();
        
        // Header
        writeln!(output, "{:<30} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
            "filename", "access", "modify", "attrib", "close_w", "close_nw", "open", "move_fr", "move_to", "create", "delete", "del_slf", "move_slf", "unmount", "total").unwrap();
        
        // Separator
        writeln!(output, "{}", "-".repeat(150)).unwrap();
        
        // Get sorted file stats
        let file_stats = if let Some(event_mask) = sort_by {
            stats.get_sorted_file_stats_by_event(event_mask, ascending)
        } else {
            stats.get_sorted_file_stats(ascending)
        };
        
        // Format each file's statistics
        for file_stat in file_stats {
            // Skip files with zero total if show_zeros is false
            if !show_zeros && file_stat.total == 0 {
                continue;
            }
            
            let filename = file_stat.path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| file_stat.path.to_string_lossy());
            
            writeln!(output, "{:<30} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
                filename,
                if show_zeros || file_stat.access > 0 { file_stat.access.to_string() } else { "".to_string() },
                if show_zeros || file_stat.modify > 0 { file_stat.modify.to_string() } else { "".to_string() },
                if show_zeros || file_stat.attrib > 0 { file_stat.attrib.to_string() } else { "".to_string() },
                if show_zeros || file_stat.close_write > 0 { file_stat.close_write.to_string() } else { "".to_string() },
                if show_zeros || file_stat.close_nowrite > 0 { file_stat.close_nowrite.to_string() } else { "".to_string() },
                if show_zeros || file_stat.open > 0 { file_stat.open.to_string() } else { "".to_string() },
                if show_zeros || file_stat.moved_from > 0 { file_stat.moved_from.to_string() } else { "".to_string() },
                if show_zeros || file_stat.moved_to > 0 { file_stat.moved_to.to_string() } else { "".to_string() },
                if show_zeros || file_stat.create > 0 { file_stat.create.to_string() } else { "".to_string() },
                if show_zeros || file_stat.delete > 0 { file_stat.delete.to_string() } else { "".to_string() },
                if show_zeros || file_stat.delete_self > 0 { file_stat.delete_self.to_string() } else { "".to_string() },
                if show_zeros || file_stat.move_self > 0 { file_stat.move_self.to_string() } else { "".to_string() },
                if show_zeros || file_stat.unmount > 0 { file_stat.unmount.to_string() } else { "".to_string() },
                if show_zeros || file_stat.total > 0 { file_stat.total.to_string() } else { "".to_string() }
            ).unwrap();
        }
        
        // Total row
        writeln!(output, "{}", "-".repeat(150)).unwrap();
        writeln!(output, "{:<30} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
            "total",
            stats.total_access,
            stats.total_modify,
            stats.total_attrib,
            stats.total_close_write,
            stats.total_close_nowrite,
            stats.total_open,
            stats.total_moved_from,
            stats.total_moved_to,
            stats.total_create,
            stats.total_delete,
            stats.total_delete_self,
            stats.total_move_self,
            stats.total_unmount,
            stats.total_events
        ).unwrap();
        
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventMask};
    use std::path::PathBuf;
    
    #[test]
    fn test_event_formatter() {
        let event = Event::new(
            1,
            EventMask::MODIFY,
            0,
            Some("test.txt".to_string()),
            PathBuf::from("/tmp/test.txt")
        );
        
        let formatter = EventFormatter::new();
        
        // Test filename format
        let result = formatter.format_event(&event, "%f").unwrap();
        assert_eq!(result, "test.txt");
        
        // Test event format
        let result = formatter.format_event(&event, "%e").unwrap();
        assert_eq!(result, "MODIFY");
        
        // Test watch descriptor format
        let result = formatter.format_event(&event, "%w").unwrap();
        assert_eq!(result, "1");
    }
    
    #[test]
    fn test_csv_formatter() {
        let event = Event::new(
            1,
            EventMask::MODIFY,
            0,
            Some("test.txt".to_string()),
            PathBuf::from("/tmp/test.txt")
        );
        
        let formatter = EventFormatter::new();
        let result = formatter.format_csv(&event);
        assert!(result.contains("test.txt"));
        assert!(result.contains("MODIFY"));
    }
    
    #[test]
    fn test_default_formatter() {
        let event = Event::new(
            1,
            EventMask::MODIFY,
            0,
            Some("test.txt".to_string()),
            PathBuf::from("/tmp/test.txt")
        );
        
        let formatter = EventFormatter::new();
        let result = formatter.format_default(&event);
        assert!(result.contains("test.txt"));
        assert!(result.contains("MODIFY"));
    }
}