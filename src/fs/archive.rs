use std::path::Path;

use crate::util::find_sevenzip;

// ── Shell quoting ─────────────────────────────────────────────────────────────

/// Wrap a path in shell quotes so it can be safely embedded in a command string
/// passed to `sh -c` (Unix) or `cmd /C` (Windows).
pub fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        // Double-quote; escape any embedded double-quotes by doubling them.
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    #[cfg(not(windows))]
    {
        // Single-quote; escape any embedded single-quotes using the '"'"' trick.
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// ── Tool probing ──────────────────────────────────────────────────────────────

/// Return true if `tool` can be found and spawned on the current PATH.
/// Uses `--version` as a harmless probe argument; the exit code is irrelevant -
/// a successful spawn means the binary exists.
fn tool_available(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

// ── Command resolution ────────────────────────────────────────────────────────

/// Build the shell command string to extract `archive` into `dest`.
///
/// Selects the best available tool for each format and OS:
///
/// | Format          | Linux / macOS        | Windows 10+          |
/// |-----------------|----------------------|----------------------|
/// | .zip            | unzip -> 7z          | tar (built-in) -> 7z |
/// | .tar / .tar.*   | tar (built-in)       | tar (built-in)       |
/// | .tgz / .tbz2    | tar (built-in)       | tar (built-in)       |
/// | .7z             | 7z / 7za / 7zz       | 7z / 7za / 7zz       |
/// | .rar            | unrar -> 7z          | unrar -> 7z          |
/// | .gz/.bz2/.xz    | gzip/bzip2/xz -> 7z  | 7z                   |
///
/// Returns `Err` with a human-readable message when no suitable tool is found.
pub fn resolve_unpack_command(archive: &Path, dest: &Path) -> Result<String, String> {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let aq = shell_quote(archive);
    let dq = shell_quote(dest);

    // ── TAR variants - tar is present everywhere ──────────────────────────────
    // On Windows, paths like C:\... make tar think `C` is a remote host.
    // --force-local tells tar to treat the colon as part of a local path.
    if name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tar.xz")
    {
        #[cfg(windows)]
        return Ok(format!("tar --force-local -xf {} -C {}", aq, dq));
        #[cfg(not(windows))]
        return Ok(format!("tar -xf {} -C {}", aq, dq));
    }

    // ── ZIP ───────────────────────────────────────────────────────────────────
    if name.ends_with(".zip") {
        #[cfg(windows)]
        {
            // tar on Windows 10+ understands ZIP natively.
            // --force-local prevents tar from treating C: as a remote host.
            return Ok(format!("tar --force-local -xf {} -C {}", aq, dq));
        }
        #[cfg(not(windows))]
        {
            if tool_available("unzip") {
                return Ok(format!("unzip {} -d {}", aq, dq));
            }
            if let Some(bin) = find_sevenzip() {
                return Ok(format!("{} x {} -o{}", bin, aq, dq));
            }
            return Err(
                "unzip not found - install it with your package manager (e.g. apt install unzip)"
                    .to_string(),
            );
        }
    }

    // ── 7Z ────────────────────────────────────────────────────────────────────
    if name.ends_with(".7z") {
        if let Some(bin) = find_sevenzip() {
            return Ok(format!("{} x {} -o{}", bin, aq, dq));
        }
        return Err(
            "7z not found on PATH - install 7-Zip (p7zip-full on Debian/Ubuntu, p7zip on macOS via Homebrew)"
                .to_string(),
        );
    }

    // ── RAR ───────────────────────────────────────────────────────────────────
    if name.ends_with(".rar") {
        if tool_available("unrar") {
            // -op sets the output path (no space between flag and path)
            return Ok(format!("unrar x {} -op{}", aq, dq));
        }
        if let Some(bin) = find_sevenzip() {
            // 7z can extract most RAR archives
            return Ok(format!("{} x {} -o{}", bin, aq, dq));
        }
        return Err(
            "unrar not found - install it with your package manager (e.g. apt install unrar)"
                .to_string(),
        );
    }

    // ── Single compressed files (.gz / .bz2 / .xz not inside a tarball) ──────
    // Decompress to dest/<original-stem>
    let stem = archive
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let dest_file_q = shell_quote(&dest.join(&stem));

    if name.ends_with(".gz") {
        if tool_available("gzip") {
            return Ok(format!("gzip -dc {} > {}", aq, dest_file_q));
        }
        if let Some(bin) = find_sevenzip() {
            return Ok(format!("{} x {} -o{}", bin, aq, dq));
        }
        return Err("gzip not found".to_string());
    }

    if name.ends_with(".bz2") {
        if tool_available("bzip2") {
            return Ok(format!("bzip2 -dc {} > {}", aq, dest_file_q));
        }
        if let Some(bin) = find_sevenzip() {
            return Ok(format!("{} x {} -o{}", bin, aq, dq));
        }
        return Err("bzip2 not found".to_string());
    }

    if name.ends_with(".xz") {
        if tool_available("xz") {
            return Ok(format!("xz -dc {} > {}", aq, dest_file_q));
        }
        if let Some(bin) = find_sevenzip() {
            return Ok(format!("{} x {} -o{}", bin, aq, dq));
        }
        return Err("xz not found".to_string());
    }

    Err(format!("unsupported archive format: {}", name))
}
