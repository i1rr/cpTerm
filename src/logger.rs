use std::sync::Mutex;

use log::{Level, Log, Metadata, Record};

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

/// Drain and print any collected warnings/errors to stderr.
/// Returns the count of messages printed.
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

    eprintln!("\n--- session log ({} issue{}) ---", entries.len(), if entries.len() == 1 { "" } else { "s" });
    for entry in &entries {
        eprintln!("{}: {}", entry.level, entry.message);
    }

    entries.len()
}
