use std::path::PathBuf;

use std::time::{SystemTime, UNIX_EPOCH};

use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};

/// A name from an input field or remote server must stay inside its destination.
pub fn valid_filename(name: &str) -> bool {
    let valid = !(name.is_empty()
        || matches!(name, "." | "..")
        || name.contains('/')
        || name.chars().any(char::is_control));
    #[cfg(windows)]
    let valid = {
        let base = name
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_ascii_uppercase();
        valid
            && !name.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
            && !name.ends_with([' ', '.'])
            && !matches!(
                base.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            && !["COM", "LPT"].iter().any(|p| {
                base.strip_prefix(p)
                    .is_some_and(|n| n.chars().count() == 1 && "0123456789¹²³".contains(n))
            })
    };
    valid
}

/// Canonicalize a path, stripping the Windows `\\?\` extended-length prefix.
pub fn clean_canonicalize(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    Ok(strip_unc_prefix(resolved))
}

/// Strip the `\\?\` prefix that Windows canonicalize produces.
pub fn strip_unc_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(s) = path.to_str() {
            if let Some(share) = s.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{share}"));
            }
            if let Some(drive) = s.strip_prefix(r"\\?\")
                && drive.as_bytes().get(1..3) == Some(b":\\")
            {
                return PathBuf::from(drive);
            }
        }
    }
    path
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_date(dt: &SystemTime) -> String {
    // The date column is 12 cells wide, including the UTC fallback marker.
    format_local_time(
        *dt,
        format_description!("[month]-[day] [hour]:[minute]"),
        "Z",
    )
}

pub(crate) fn format_log_time(at: SystemTime, milliseconds: bool) -> String {
    let description = if milliseconds {
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]")
    } else {
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")
    };
    format_local_time(at, description, " UTC")
}

fn format_local_time(at: SystemTime, description: &[FormatItem<'_>], utc_suffix: &str) -> String {
    // Keep timestamps in std until presentation. Preserve subsecond precision and
    // dates before 1970 without unchecked calendar conversions.
    let nanos = match at.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(error) => -(error.duration().as_nanos() as i128),
    };
    let Ok(utc) = OffsetDateTime::from_unix_timestamp_nanos(nanos) else {
        return "?".into();
    };
    let local = UtcOffset::local_offset_at(utc)
        .ok()
        .and_then(|offset| utc.checked_to_offset(offset));
    let (datetime, suffix) = match local {
        Some(datetime) => (datetime, ""),
        // An unavailable system timezone must not silently look like local time.
        None => (utc, utc_suffix),
    };
    match datetime.format(description) {
        Ok(value) => value + suffix,
        Err(_) => "?".into(),
    }
}

/// Return true if the file at `path` appears to be a text file that can be
/// opened in the embedded editor. Uses a two-step check:
/// 1. Known binary extensions are rejected immediately.
/// 2. The first 8 KB of the file are scanned for null bytes - a reliable
///    heuristic used by git, file(1), and most editors.
pub fn is_text_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    } // Opening a FIFO for reading can block indefinitely.
    // Known binary extensions - skip the content probe for speed.
    const BINARY_EXTS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "tif", "tiff", "mp3", "mp4",
        "avi", "mkv", "mov", "flv", "wav", "flac", "ogg", "aac", "wma", "wmv", "exe", "dll", "so",
        "dylib", "bin", "obj", "o", "a", "lib", "class", "pdf", "doc", "docx", "xls", "xlsx",
        "ppt", "pptx", "zip", "7z", "rar", "tar", "gz", "bz2", "xz", "tgz", "tbz2", "zst", "lz4",
        "iso", "img", "dmg", "msi", "deb", "rpm", "ttf", "otf", "woff", "woff2", "eot", "sqlite",
        "db", "mdb", "pyc", "pyo", "wasm",
    ];

    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if BINARY_EXTS.contains(&ext.as_str()) {
            return false;
        }
    }

    // Read the first 8 KB and check for null bytes.
    match std::fs::File::open(path) {
        Ok(mut f) => {
            use std::io::Read;
            let mut buf = [0u8; 8192];
            let n = match f.read(&mut buf) {
                Ok(n) => n,
                Err(_) => return false,
            };
            // Empty files are valid text files.
            if n == 0 {
                return true;
            }
            !buf[..n].contains(&0)
        }
        Err(_) => false,
    }
}

/// Return true if the filename looks like a supported archive.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".zip")
        || lower.ends_with(".7z")
        || lower.ends_with(".rar")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".bz2")
        || lower.ends_with(".xz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.xz")
}

/// Find the 7-Zip binary on PATH or in well-known install locations.
/// Tries `7z`, `7za`, and `7zz` on PATH first, then checks common Windows
/// install directories (7-Zip, PeaZip, NanaZip, etc.).
pub fn find_sevenzip() -> Option<String> {
    // 1. Try PATH first
    for candidate in &["7z", "7za", "7zz"] {
        if std::process::Command::new(candidate)
            .arg("i")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return Some((*candidate).to_string());
        }
    }

    // 2. On Windows, check well-known install directories
    #[cfg(windows)]
    {
        let program_files: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|var| std::env::var(var).ok())
            .collect();

        // Paths relative to Program Files where 7z.exe might live
        let relative_paths: &[&str] = &[
            r"7-Zip\7z.exe",
            r"PeaZip\res\bin\7z\7z.exe",
            r"PeaZip\res\7z\7z.exe",
            r"NanaZip\7z.exe",
        ];

        for pf in &program_files {
            for rel in relative_paths {
                let full = PathBuf::from(pf).join(rel);
                if full.exists() {
                    return Some(full.to_string_lossy().into_owned());
                }
            }
        }

        // Also check LocalAppData for user-level installs
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            for rel in &[r"PeaZip\res\bin\7z\7z.exe", r"PeaZip\res\7z\7z.exe"] {
                let full = PathBuf::from(&local).join(rel);
                if full.exists() {
                    return Some(full.to_string_lossy().into_owned());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn log_timestamps_preserve_milliseconds_on_both_sides_of_epoch() {
        for (timestamp, fraction) in [
            (UNIX_EPOCH + Duration::from_millis(123), ".123"),
            (UNIX_EPOCH - Duration::from_millis(1), ".999"),
        ] {
            let precise = format_log_time(timestamp, true);
            let seconds = format_log_time(timestamp, false);
            let precise = precise.strip_suffix(" UTC").unwrap_or(&precise);
            let seconds = seconds.strip_suffix(" UTC").unwrap_or(&seconds);
            assert_eq!(precise, format!("{seconds}{fraction}"));
            assert_eq!(seconds.len(), 19);
        }
    }

    #[test]
    fn extreme_file_dates_do_not_panic_during_local_conversion() {
        let last_second = UNIX_EPOCH + Duration::from_secs(253_402_300_799);
        let rendered = format_date(&last_second);
        assert_ne!(rendered, "?");
        assert!(
            rendered.len() <= 12,
            "date and timezone marker must fit the column"
        );
        let outside_calendar = last_second + Duration::from_secs(1);
        assert_eq!(format_date(&outside_calendar), "?");
    }
}
