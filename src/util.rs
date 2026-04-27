use std::path::PathBuf;

use chrono::{DateTime, Local};

/// Canonicalize a path, stripping the Windows `\\?\` extended-length prefix.
pub fn clean_canonicalize(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    Ok(strip_unc_prefix(resolved))
}

/// Strip the `\\?\` prefix that Windows canonicalize produces.
pub fn strip_unc_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
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

pub fn format_date(dt: &DateTime<Local>) -> String {
    dt.format("%m-%d %H:%M").to_string()
}

/// Return true if the file at `path` appears to be a text file that can be
/// opened in the embedded editor. Uses a two-step check:
/// 1. Known binary extensions are rejected immediately.
/// 2. The first 8 KB of the file are scanned for null bytes - a reliable
///    heuristic used by git, file(1), and most editors.
pub fn is_text_file(path: &std::path::Path) -> bool {
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
