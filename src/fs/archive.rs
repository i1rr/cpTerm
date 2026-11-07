use std::path::{Path, PathBuf};

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
        if name.ends_with(suffix) && tool_available(tool) {
            let stem = archive
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| crate::util::valid_filename(s))
                .ok_or("Invalid output filename")?;
            return Ok(UnpackCommand::Decompress {
                program: tool.to_owned(),
                archive: archive_arg.to_owned(),
                output: dest.join(stem),
            });
        }
    }
    if [".rar", ".zip", ".7z", ".gz", ".bz2", ".xz"]
        .iter()
        .any(|s| name.ends_with(s))
    {
        let bin = find_sevenzip()
            .ok_or("No suitable extractor found; install 7z or a format-specific extractor")?;
        return direct(
            &bin,
            &["x", "-aos", &format!("-o{dest_arg}"), "--", archive_arg],
        );
    }
    Err(format!("unsupported archive format: {name}"))
}

/// A failed decoder must not leave a partial file or follow an existing output symlink.
pub async fn decompress(program: String, archive: String, output: PathBuf) -> Result<(), String> {
    let tmp = tempfile::NamedTempFile::new_in(output.parent().ok_or("Missing output directory")?)
        .map_err(|e| e.to_string())?;
    let status = tokio::process::Command::new(program)
        .args(["-dc", "--", &archive])
        .stdout(tmp.reopen().map_err(|e| e.to_string())?)
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("Decoder failed: {status}"));
    }
    tmp.persist_noclobber(&output)
        .map_err(|e| format!("Cannot create {}: {e}", output.display()))?;
    Ok(())
}
