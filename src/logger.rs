use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use log::{Level, Log, Metadata, Record};

const MAX_LOG_LINES: usize = 500;

static BUFFER: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());

struct LogEntry {
    level: Level,
    message: String,
}

struct MemoryLogger;

impl Log for MemoryLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if let Ok(mut buf) = BUFFER.lock() {
                buf.push(LogEntry {
                    level: record.level(),
                    message: format!("[{}] {}", record.target(), record.args()),
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

fn log_path() -> Option<PathBuf> {
    let dir = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok()?;
    Some(dir.join("cpt").join("cpt.log"))
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

    // Append to log file
    if let Some(path) = log_path() {
        let _ = write_to_file(&path, &entries);
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
