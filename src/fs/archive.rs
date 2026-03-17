use std::path::Path;

use crate::util::find_sevenzip;

// ── Shell quoting ─────────────────────────────────────────────────────────────

/// Wrap a path in shell quotes so it can be safely embedded in a command string
/// passed to `sh -c` (Unix) or `cmd /C` (Windows).
pub fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// ── Unpack command ───────────────────────────────────────────────────────────

/// A resolved unpack command: either a shell string (Unix) or a direct
/// program + args list (Windows, to bypass cmd /C quoting issues).
#[allow(dead_code)]
#[derive(Debug)]
pub enum UnpackCommand {
    /// A shell command string to pass to `sh -c` or `cmd /C`.
    Shell(String),
    /// A program and its arguments - spawned directly, no shell.
    Direct { program: String, args: Vec<String> },
}

impl UnpackCommand {
    /// Human-readable display string for the task window title.
    pub fn display(&self) -> String {
        match self {
            UnpackCommand::Shell(s) => s.clone(),
            UnpackCommand::Direct { program, args } => {
                format!(
                    "{} {}",
                    program,
                    args.iter()
                        .map(|a| if a.contains(' ') {
                            format!("\"{}\"", a)
                        } else {
                            a.clone()
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
        }
    }
}

// ── Tool probing ──────────────────────────────────────────────────────────────

/// Return true if `tool` can be found and spawned on the current PATH.
fn tool_available(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

// ── Command resolution ────────────────────────────────────────────────────────

/// Build the command to extract `archive` into `dest`.
///
/// On Unix, returns a shell string for `sh -c`.
/// On Windows, returns a direct program + args to avoid cmd /C quoting issues.
///
/// Returns `Err` with a human-readable message when no suitable tool is found.
pub fn resolve_unpack_command(archive: &Path, dest: &Path) -> Result<UnpackCommand, String> {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let archive_str = archive.to_string_lossy().to_string();
    let dest_str = dest.to_string_lossy().to_string();
    // Suppress unused warnings on Unix (these are used in #[cfg(windows)] blocks).
    let _ = (&archive_str, &dest_str);

    // ── TAR variants ─────────────────────────────────────────────────────────
    if name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tar.xz")
    {
        #[cfg(windows)]
        return Ok(UnpackCommand::Direct {
            program: "tar".to_string(),
            args: vec![
                "--force-local".to_string(),
                "-xf".to_string(),
                archive_str,
                "-C".to_string(),
                dest_str,
            ],
        });
        #[cfg(not(windows))]
        {
            let aq = shell_quote(archive);
            let dq = shell_quote(dest);
            return Ok(UnpackCommand::Shell(format!("tar -xf {} -C {}", aq, dq)));
        }
    }

    // ── ZIP ───────────────────────────────────────────────────────────────────
    if name.ends_with(".zip") {
        #[cfg(windows)]
        {
            // Use PowerShell Expand-Archive which handles Windows paths natively.
            return Ok(UnpackCommand::Direct {
                program: "powershell".to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    format!(
                        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                        archive_str.replace('\'', "''"),
                        dest_str.replace('\'', "''"),
                    ),
                ],
            });
        }
        #[cfg(not(windows))]
        {
            let aq = shell_quote(archive);
            let dq = shell_quote(dest);
            if tool_available("unzip") {
                return Ok(UnpackCommand::Shell(format!("unzip {} -d {}", aq, dq)));
            }
            if let Some(bin) = find_sevenzip() {
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
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
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: bin,
                args: vec![
                    "x".to_string(),
                    archive_str,
                    format!("-o{}", dest_str),
                    "-y".to_string(),
                ],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
            }
        }
        return Err(
            "7z not found on PATH - install 7-Zip (p7zip-full on Debian/Ubuntu, p7zip on macOS via Homebrew)"
                .to_string(),
        );
    }

    // ── RAR ───────────────────────────────────────────────────────────────────
    if name.ends_with(".rar") {
        if tool_available("unrar") {
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: "unrar".to_string(),
                args: vec!["x".to_string(), archive_str, format!("-op{}", dest_str)],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("unrar x {} -op{}", aq, dq)));
            }
        }
        if let Some(bin) = find_sevenzip() {
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: bin,
                args: vec![
                    "x".to_string(),
                    archive_str,
                    format!("-o{}", dest_str),
                    "-y".to_string(),
                ],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
            }
        }
        return Err(
            "unrar not found - install it with your package manager (e.g. apt install unrar)"
                .to_string(),
        );
    }

    // ── Single compressed files (.gz / .bz2 / .xz not inside a tarball) ──────
    let stem = archive
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());

    if name.ends_with(".gz") {
        #[cfg(not(windows))]
        {
            if tool_available("gzip") {
                let aq = shell_quote(archive);
                let dest_file_q = shell_quote(&dest.join(&stem));
                return Ok(UnpackCommand::Shell(format!(
                    "gzip -dc {} > {}",
                    aq, dest_file_q
                )));
            }
        }
        if let Some(bin) = find_sevenzip() {
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: bin,
                args: vec![
                    "x".to_string(),
                    archive_str,
                    format!("-o{}", dest_str),
                    "-y".to_string(),
                ],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
            }
        }
        return Err("gzip not found".to_string());
    }

    if name.ends_with(".bz2") {
        #[cfg(not(windows))]
        {
            if tool_available("bzip2") {
                let aq = shell_quote(archive);
                let dest_file_q = shell_quote(&dest.join(&stem));
                return Ok(UnpackCommand::Shell(format!(
                    "bzip2 -dc {} > {}",
                    aq, dest_file_q
                )));
            }
        }
        if let Some(bin) = find_sevenzip() {
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: bin,
                args: vec![
                    "x".to_string(),
                    archive_str,
                    format!("-o{}", dest_str),
                    "-y".to_string(),
                ],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
            }
        }
        return Err("bzip2 not found".to_string());
    }

    if name.ends_with(".xz") {
        #[cfg(not(windows))]
        {
            if tool_available("xz") {
                let aq = shell_quote(archive);
                let dest_file_q = shell_quote(&dest.join(&stem));
                return Ok(UnpackCommand::Shell(format!(
                    "xz -dc {} > {}",
                    aq, dest_file_q
                )));
            }
        }
        if let Some(bin) = find_sevenzip() {
            #[cfg(windows)]
            return Ok(UnpackCommand::Direct {
                program: bin,
                args: vec![
                    "x".to_string(),
                    archive_str,
                    format!("-o{}", dest_str),
                    "-y".to_string(),
                ],
            });
            #[cfg(not(windows))]
            {
                let aq = shell_quote(archive);
                let dq = shell_quote(dest);
                return Ok(UnpackCommand::Shell(format!("{} x {} -o{}", bin, aq, dq)));
            }
        }
        return Err("xz not found".to_string());
    }

    Err(format!("unsupported archive format: {}", name))
}
