//! inotifywatch - Monitor filesystem events using inotify
//!
//! This is a Rust port of the original inotifywatch C++ tool.

use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::{Arg, ArgAction, Command};
use inotifytools::{Config, EventMask, InotifyTools, str_to_events};
use signal_hook::{consts::{SIGINT, SIGHUP, SIGTERM, SIGUSR1}, iterator::Signals};

const EXIT_SUCCESS: i32 = 0;
const EXIT_FAILURE: i32 = 1;
const EXIT_TIMEOUT: i32 = 2;

/// Application configuration
#[derive(Debug)]
struct AppConfig {
    events: EventMask,
    verbose: bool,
    timeout: Option<Duration>,
    recursive: bool,
    no_dereference: bool,
    fromfile: Option<String>,
    exclude_regex: Option<String>,
    exclude_iregex: Option<String>,
    include_regex: Option<String>,
    include_iregex: Option<String>,
    zero: bool,
    fanotify: bool,
    filesystem: bool,
    sort_ascending: Option<String>,
    sort_descending: Option<String>,
    paths: Vec<PathBuf>,
}

static DONE: AtomicBool = AtomicBool::new(false);
static PRINT_STATS: AtomicBool = AtomicBool::new(false);

fn main() {
    let result = run();
    process::exit(result);
}

fn run() -> i32 {
    let config = match parse_args() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error parsing arguments: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Set up signal handling
    setup_signal_handlers();

    // Initialize inotify tools
    let inotify_config = Config {
        fanotify: config.fanotify,
        filesystem_watch: config.filesystem,
        verbose: config.verbose,
    };

    let mut tools = match InotifyTools::new(inotify_config) {
        Ok(tools) => tools,
        Err(e) => {
            eprintln!("Failed to initialize inotify: {}", e);
            return EXIT_FAILURE;
        }
    };

    // Set up regex filters
    if let Some(ref pattern) = config.exclude_regex {
        if let Err(e) = tools.set_exclude_regex(pattern) {
            eprintln!("Error in exclude regular expression: {}", e);
            return EXIT_FAILURE;
        }
    }

    if let Some(ref pattern) = config.include_regex {
        if let Err(e) = tools.set_include_regex(pattern) {
            eprintln!("Error in include regular expression: {}", e);
            return EXIT_FAILURE;
        }
    }

    // Process file list
    let watch_paths = if config.paths.is_empty() {
        if let Some(ref fromfile) = config.fromfile {
            match read_paths_from_file(fromfile) {
                Ok(paths) => paths,
                Err(e) => {
                    eprintln!("Error reading paths from file: {}", e);
                    return EXIT_FAILURE;
                }
            }
        } else {
            eprintln!("No files specified to watch!");
            return EXIT_FAILURE;
        }
    } else {
        config.paths.clone()  // Clone to avoid partial move
    };

    if watch_paths.is_empty() {
        eprintln!("No files specified to watch!");
        return EXIT_FAILURE;
    }

    // Set up watches
    let events = if config.events.mask() == 0 {
        EventMask::ALL_EVENTS
    } else {
        config.events
    };

    let mut watch_events = events;
    if config.recursive {
        watch_events = watch_events | EventMask::CREATE | EventMask::MOVED_TO | EventMask::MOVED_FROM;
    }

    if config.fanotify {
        watch_events = watch_events | EventMask::ISDIR;
    }

    if config.no_dereference {
        // Note: IN_DONT_FOLLOW equivalent would need to be implemented
        // in the underlying inotify wrapper
    }

    let mut num_watches = 0;
    eprintln!("Establishing watches...");

    for path in &watch_paths {
        let result = if config.recursive {
            tools.watch_recursively(path, watch_events)
        } else {
            tools.watch_file(path, watch_events).map(|wd| vec![wd])
        };

        match result {
            Ok(wds) => {
                num_watches += wds.len();
                if config.verbose {
                    eprintln!("Watching {} (watches: {})", path.display(), wds.len());
                }
            }
            Err(e) => {
                eprintln!("Couldn't watch {}: {}", path.display(), e);
                return EXIT_FAILURE;
            }
        }
    }

    if config.verbose {
        eprintln!("Watches established ({} watches).", num_watches);
    }

    // Set up timeout alarm if specified
    if let Some(timeout) = config.timeout {
        let timeout_secs = timeout.as_secs();
        if timeout_secs > 0 {
            if config.verbose {
                eprintln!("Will listen for events for {} seconds.", timeout_secs);
            }
            
            // Set up timeout handling
            let done_clone = Arc::new(AtomicBool::new(false));
            let done_ref = done_clone.clone();
            
            std::thread::spawn(move || {
                std::thread::sleep(timeout);
                done_ref.store(true, Ordering::SeqCst);
            });
        }
    }

    // Main event loop
    let mut moved_from: Option<PathBuf> = None;
    
    loop {
        if DONE.load(Ordering::SeqCst) {
            break;
        }

        // Check if we should print stats (SIGUSR1)
        if PRINT_STATS.load(Ordering::SeqCst) {
            print_stats(&tools, &config);
            PRINT_STATS.store(false, Ordering::SeqCst);
        }

        match tools.next_event(Some(Duration::from_millis(100))) {
            Ok(Some(event)) => {
                // Handle MOVED_FROM/MOVED_TO logic for recursive watching
                if config.recursive && !config.filesystem {
                    if event.mask.contains(EventMask::MOVED_FROM) {
                        moved_from = Some(event.path.clone());
                    } else if let Some(ref from_path) = moved_from {
                        if !event.mask.contains(EventMask::MOVED_TO) {
                            // File was moved outside of watched tree
                            let _ = tools.remove_watch_by_filename(from_path);
                        }
                        moved_from = None;
                    }

                    // Add watches for newly created directories
                    if (event.mask.contains(EventMask::CREATE) || 
                        (moved_from.is_none() && event.mask.contains(EventMask::MOVED_TO))) 
                        && event.is_dir() {
                        let _ = tools.watch_recursively(&event.path, watch_events);
                    }
                }
                // Note: Statistics are automatically collected by the InotifyTools instance
            }
            Ok(None) => {
                // Timeout on read - continue loop
                continue;
            }
            Err(e) => {
                eprintln!("Error reading events: {}", e);
                return EXIT_FAILURE;
            }
        }
    }

    // Print final statistics
    print_final_stats(&tools, &config)
}

fn setup_signal_handlers() {
    std::thread::spawn(|| {
        let mut signals = Signals::new(&[SIGINT, SIGHUP, SIGTERM, SIGUSR1])
            .expect("Failed to register signal handlers");
        
        for sig in signals.forever() {
            match sig {
                SIGINT | SIGHUP | SIGTERM => {
                    DONE.store(true, Ordering::SeqCst);
                    break;
                }
                SIGUSR1 => {
                    PRINT_STATS.store(true, Ordering::SeqCst);
                }
                _ => {}
            }
        }
    });
}

fn print_stats(tools: &InotifyTools, config: &AppConfig) {
    if let Ok(stats) = tools.get_statistics() {
        let table = inotifytools::format::TableFormatter::format_stats_table(
            &stats,
            None, // No specific sort by event type for intermediate stats
            false, // Descending order
            config.zero
        );
        println!("{}", table);
        println!(); // Add extra newline
    }
}

fn print_final_stats(tools: &InotifyTools, config: &AppConfig) -> i32 {
    let stats = match tools.get_statistics() {
        Ok(stats) => stats,
        Err(e) => {
            eprintln!("Error getting statistics: {}", e);
            return EXIT_FAILURE;
        }
    };

    if stats.total_events == 0 {
        eprintln!("No events occurred.");
        return EXIT_SUCCESS;
    }

    // Determine sort order
    let (sort_event, ascending) = if let Some(ref sort_field) = config.sort_ascending {
        (parse_sort_field(sort_field), true)
    } else if let Some(ref sort_field) = config.sort_descending {
        (parse_sort_field(sort_field), false)
    } else {
        (None, false) // Default to descending by total
    };

    let table = inotifytools::format::TableFormatter::format_stats_table(
        &stats,
        sort_event,
        ascending,
        config.zero
    );

    print!("{}", table);

    EXIT_SUCCESS
}

fn parse_sort_field(field: &str) -> Option<EventMask> {
    match field.to_lowercase().as_str() {
        "access" => Some(EventMask::ACCESS),
        "modify" => Some(EventMask::MODIFY),
        "attrib" => Some(EventMask::ATTRIB),
        "close_write" => Some(EventMask::CLOSE_WRITE),
        "close_nowrite" => Some(EventMask::CLOSE_NOWRITE),
        "close" => Some(EventMask::CLOSE),
        "open" => Some(EventMask::OPEN),
        "moved_from" => Some(EventMask::MOVED_FROM),
        "moved_to" => Some(EventMask::MOVED_TO),
        "move" => Some(EventMask::MOVE),
        "create" => Some(EventMask::CREATE),
        "delete" => Some(EventMask::DELETE),
        "delete_self" => Some(EventMask::DELETE_SELF),
        "move_self" => Some(EventMask::MOVE_SELF),
        "unmount" => Some(EventMask::UNMOUNT),
        "total" => None, // Total is handled as the default case
        _ => None,
    }
}

fn parse_args() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let app = Command::new("inotifywatch")
        .version("4.23.9.0")
        .about("Gather filesystem usage statistics using inotify")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .help("Be verbose"),
        )
        .arg(
            Arg::new("fromfile")
                .long("fromfile")
                .value_name("FILE")
                .help("Read files to watch from FILE or '-' for stdin"),
        )
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("PATTERN")
                .help("Exclude all events on files matching the extended regular expression PATTERN"),
        )
        .arg(
            Arg::new("excludei")
                .long("excludei")
                .value_name("PATTERN")
                .help("Like --exclude but case insensitive"),
        )
        .arg(
            Arg::new("include")
                .long("include")
                .value_name("PATTERN")
                .help("Exclude all events on files except the ones matching the extended regular expression PATTERN"),
        )
        .arg(
            Arg::new("includei")
                .long("includei")
                .value_name("PATTERN")
                .help("Like --include but case insensitive"),
        )
        .arg(
            Arg::new("zero")
                .short('z')
                .long("zero")
                .action(ArgAction::SetTrue)
                .help("In the final table of results, output rows and columns even if they consist only of zeros"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .action(ArgAction::SetTrue)
                .help("Watch directories recursively"),
        )
        .arg(
            Arg::new("inotify")
                .short('I')
                .long("inotify")
                .action(ArgAction::SetTrue)
                .help("Watch with inotify"),
        )
        .arg(
            Arg::new("fanotify")
                .short('F')
                .long("fanotify")
                .action(ArgAction::SetTrue)
                .help("Watch with fanotify"),
        )
        .arg(
            Arg::new("filesystem")
                .short('S')
                .long("filesystem")
                .action(ArgAction::SetTrue)
                .help("Watch entire filesystem with fanotify"),
        )
        .arg(
            Arg::new("no-dereference")
                .short('P')
                .long("no-dereference")
                .action(ArgAction::SetTrue)
                .help("Do not follow symlinks"),
        )
        .arg(
            Arg::new("timeout")
                .short('t')
                .long("timeout")
                .value_name("SECONDS")
                .help("Listen only for specified amount of time in seconds"),
        )
        .arg(
            Arg::new("event")
                .short('e')
                .long("event")
                .value_name("EVENT")
                .action(ArgAction::Append)
                .help("Listen for specific event(s). If omitted, all events are listened for"),
        )
        .arg(
            Arg::new("ascending")
                .short('a')
                .long("ascending")
                .value_name("EVENT")
                .help("Sort ascending by a particular event, or 'total'"),
        )
        .arg(
            Arg::new("descending")
                .short('d')
                .long("descending")
                .value_name("EVENT")
                .help("Sort descending by a particular event, or 'total'"),
        )
        .arg(
            Arg::new("files")
                .value_name("FILE")
                .action(ArgAction::Append)
                .help("Files or directories to watch"),
        );

    let matches = app.try_get_matches()?;

    // Parse events
    let events = if let Some(event_list) = matches.get_many::<String>("event") {
        let event_strings: Vec<&str> = event_list.map(|s| s.as_str()).collect();
        str_to_events(&event_strings.join(","))?
    } else {
        EventMask::new(0) // Will be set to ALL_EVENTS later if still 0
    };

    // Parse timeout
    let timeout = if let Some(timeout_str) = matches.get_one::<String>("timeout") {
        let secs: u64 = timeout_str.parse()?;
        if secs == 0 {
            None
        } else {
            Some(Duration::from_secs(secs))
        }
    } else {
        None
    };

    // Parse paths
    let paths = if let Some(file_list) = matches.get_many::<String>("files") {
        file_list.map(|s| PathBuf::from(s)).collect()
    } else {
        Vec::new()
    };

    Ok(AppConfig {
        events,
        verbose: matches.get_flag("verbose"),
        timeout,
        recursive: matches.get_flag("recursive"),
        no_dereference: matches.get_flag("no-dereference"),
        fromfile: matches.get_one::<String>("fromfile").cloned(),
        exclude_regex: matches.get_one::<String>("exclude").cloned(),
        exclude_iregex: matches.get_one::<String>("excludei").cloned(),
        include_regex: matches.get_one::<String>("include").cloned(),
        include_iregex: matches.get_one::<String>("includei").cloned(),
        zero: matches.get_flag("zero"),
        fanotify: matches.get_flag("fanotify"),
        filesystem: matches.get_flag("filesystem"),
        sort_ascending: matches.get_one::<String>("ascending").cloned(),
        sort_descending: matches.get_one::<String>("descending").cloned(),
        paths,
    })
}

fn read_paths_from_file(filename: &str) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = if filename == "-" {
        return Err("Reading from stdin not yet implemented".into());
    } else {
        File::open(filename)?
    };

    let reader = BufReader::new(file);
    let mut paths = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            paths.push(PathBuf::from(trimmed));
        }
    }

    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sort_field() {
        assert_eq!(parse_sort_field("access"), Some(EventMask::ACCESS));
        assert_eq!(parse_sort_field("modify"), Some(EventMask::MODIFY));
        assert_eq!(parse_sort_field("total"), None);
        assert_eq!(parse_sort_field("invalid"), None);
    }
}