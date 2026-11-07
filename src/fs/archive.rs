use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};

use crate::util::find_sevenzip;

#[derive(Debug)]
pub enum UnpackCommand {
    Direct {
        program: String,
        args: Vec<String>,
    },
    Decompress {
        program: String,
        archive: String,
        output: PathBuf,
    },
}

impl UnpackCommand {
    pub fn display(&self) -> String {
        match self {
            Self::Direct { program, args } => format!(
                "{} {}",
                program,
                args.iter()
                    .map(|s| {
                        if s.contains(' ') {
                            format!("{s:?}")
                        } else {
                            s.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Decompress {
                program,
                archive,
                output,
            } => format!("{program} {archive} -> {}", output.display()),
        }
    }
}

fn tool_available(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

pub fn resolve_unpack_command(archive: &Path, dest: &Path) -> Result<UnpackCommand, String> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid archive filename")?
        .to_lowercase();
    let archive_arg = archive.to_str().ok_or("Archive path is not UTF-8")?;
    let dest_arg = dest.to_str().ok_or("Destination path is not UTF-8")?;
    let direct = |program: &str, args: &[&str]| {
        Ok(UnpackCommand::Direct {
            program: program.to_owned(),
            args: args.iter().map(|a| a.to_string()).collect(),
        })
    };
    if [".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz"]
        .iter()
        .any(|s| name.ends_with(s))
        || (cfg!(windows) && name.ends_with(".zip"))
    {
        // Windows ships bsdtar, which reads ZIP and takes literal path arguments.
        return direct("tar", &["-k", "-xf", archive_arg, "-C", dest_arg]);
    }
    if name.ends_with(".zip") && tool_available("unzip") {
        return direct("unzip", &["-n", archive_arg, "-d", dest_arg]);
    }
    if name.ends_with(".rar") && tool_available("unrar") {
        return direct(
            "unrar",
            &["x", "-o-", &format!("-op{dest_arg}"), "--", archive_arg],
        );
    }
    for (suffix, tool) in [(".gz", "gzip"), (".bz2", "bzip2"), (".xz", "xz")] {
        if name.ends_with(suffix) {
            let stem = archive
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| crate::util::valid_filename(s))
                .ok_or("Invalid output filename")?;
            let program = if tool == "gzip" || tool_available(tool) {
                tool.to_owned()
            } else {
                find_sevenzip()
                    .ok_or_else(|| format!("Install {tool} or 7z to decompress this file"))?
            };
            return Ok(UnpackCommand::Decompress {
                program,
                archive: archive_arg.to_owned(),
                output: dest.join(stem),
            });
        }
    }
    if [".rar", ".zip", ".7z"].iter().any(|s| name.ends_with(s)) {
        let bin = find_sevenzip()
            .ok_or("No suitable extractor found; install 7z or a format-specific extractor")?;
        return direct(
            &bin,
            &["x", "-aos", &format!("-o{dest_arg}"), "--", archive_arg],
        );
    }
    Err(format!("unsupported archive format: {name}"))
}

const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const EXPANSION_FLOOR: u64 = 8 * 1024 * 1024;

fn byte_budget(archive: &Path) -> io::Result<u64> {
    let input = archive.metadata()?;
    if !input.is_file() {
        return Err(io::Error::other("Archive must be a regular file"));
    }
    Ok(input
        .len()
        .saturating_mul(200)
        .clamp(EXPANSION_FLOOR, MAX_BYTES))
}

// Probe one byte beyond the allowance without writing that byte to disk.
fn copy_limited(mut input: impl Read, mut output: impl Write, limit: u64) -> io::Result<u64> {
    let bytes = io::copy(&mut input.by_ref().take(limit), &mut output)?;
    if input.read(&mut [0])? != 0 {
        return Err(io::Error::other("Archive exceeds the expanded byte limit"));
    }
    Ok(bytes)
}

fn with_stdout<T>(
    mut cmd: Command,
    read: impl FnOnce(&mut ChildStdout) -> io::Result<T>,
) -> io::Result<T> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("child stdout is piped");
    let result = read(&mut stdout);
    drop(stdout);
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let value = result?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "Archive decoder failed: {status}"
        )));
    }
    Ok(value)
}

pub async fn decompress(program: String, archive: String, output: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> io::Result<()> {
        let limit = byte_budget(Path::new(&archive))?;
        let mut tmp = tempfile::NamedTempFile::new_in(
            output
                .parent()
                .ok_or_else(|| io::Error::other("Missing output directory"))?,
        )?;
        if program == "gzip" {
            let decoder = flate2::read::MultiGzDecoder::new(File::open(&archive)?);
            copy_limited(decoder, &mut tmp, limit)?;
        } else {
            let mut cmd = Command::new(&program);
            match program.as_str() {
                "xz" => cmd.args(["--memlimit-decompress=128MiB", "-dc", "--", &archive]),
                "bzip2" => cmd.args(["-dc", "--", &archive]),
                _ => cmd.args(["x", "-so", "--", &archive]),
            };
            with_stdout(cmd, |stdout| copy_limited(stdout, &mut tmp, limit))?;
        }
        tmp.persist_noclobber(&output)
            .map_err(|e| io::Error::other(format!("Cannot create {}: {e}", output.display())))?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
