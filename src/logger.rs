use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use log::{Level, Log, Metadata, Record};

const MAX_LOG_LINES: usize = 500;

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static BUFFER: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());

struct LogEntry {
    level: Level,
    message: String,
}

struct MemoryLogger;

impl Log for MemoryLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if DEBUG_MODE.load(Ordering::Relaxed) {
            metadata.level() <= Level::Debug
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let message = format!("[{}] {}", record.target(), record.args());

            // In debug mode, also write to the log file immediately
            if DEBUG_MODE.load(Ordering::Relaxed)
                && let Some(path) = log_path()
            {
                let _ = append_line_to_file(&path, record.level(), &message);
            }

            if let Ok(mut buf) = BUFFER.lock() {
                buf.push(LogEntry {
                    level: record.level(),
                    message,
                });
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: MemoryLogger = MemoryLogger;

pub fn init() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);
}

/// Enable debug-level logging. Call after init() when --debug is passed.
pub fn enable_debug() {
    DEBUG_MODE.store(true, Ordering::Relaxed);
    log::set_max_level(log::LevelFilter::Debug);

    // Write a separator to the log file
    if let Some(path) = log_path() {
        let _ = append_line_to_file(&path, Level::Info, "--- debug session started ---");
    }
}

fn log_path() -> Option<PathBuf> {
    // Use the same directory as the session config so all cpt files are co-located.
    confy::get_configuration_file_path("cpt", Some("cpt"))
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("cpt.log")))
}

/// Append a single timestamped line to the log file.
fn append_line_to_file(path: &PathBuf, level: Level, message: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{} {}: {}", now, level, message)?;
    Ok(())
}

/// Drain collected entries, print to stderr, and append to log file.
/// The log file is capped at MAX_LOG_LINES - oldest entries are dropped.
pub fn dump() -> usize {
    let entries = {
        let Ok(mut buf) = BUFFER.lock() else {
            return 0;
        };
        std::mem::take(&mut *buf)
    };

    if entries.is_empty() {
        if DEBUG_MODE.load(Ordering::Relaxed)
            && let Some(path) = log_path()
        {
            eprintln!("debug log: {}", path.display());
        }
        return 0;
    }

    // Print to stderr
    eprintln!(
        "\n--- session log ({} issue{}) ---",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
    for entry in &entries {
        eprintln!("{}: {}", entry.level, entry.message);
    }

    // In non-debug mode, append to log file at the end (debug mode already writes in real-time)
    if !DEBUG_MODE.load(Ordering::Relaxed)
        && let Some(path) = log_path()
    {
        let _ = write_to_file(&path, &entries);
    }

    if DEBUG_MODE.load(Ordering::Relaxed)
        && let Some(path) = log_path()
    {
        eprintln!("debug log: {}", path.display());
    }

    entries.len()
}

fn write_to_file(path: &PathBuf, new_entries: &[LogEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read existing lines
    let mut lines: Vec<String> = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();

    // Append new entries with timestamp
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    for entry in new_entries {
        lines.push(format!("{} {}: {}", now, entry.level, entry.message));
    }

    // Trim to last MAX_LOG_LINES
    if lines.len() > MAX_LOG_LINES {
        lines = lines.split_off(lines.len() - MAX_LOG_LINES);
    }

    let mut file = fs::File::create(path)?;
    for line in &lines {
        writeln!(file, "{}", line)?;
    }

    Ok(())
}
