use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod stage;
use stage::Staged;

const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const EXPANSION_FLOOR: u64 = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

#[derive(Debug)]
pub struct UnpackCommand {
    archive: PathBuf,
    destination: PathBuf,
    format: Format,
}

#[derive(Debug, Clone, Copy)]
enum Format {
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    Zip,
    Libarchive,
    Gz,
    Bz2,
    Xz,
}

pub fn resolve_unpack_command(archive: &Path, dest: &Path) -> Result<UnpackCommand, String> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid archive filename")?
        .to_lowercase();
    let format = [
        (".tar.gz", Format::TarGz),
        (".tgz", Format::TarGz),
        (".tar.bz2", Format::TarBz2),
        (".tbz2", Format::TarBz2),
        (".tar.xz", Format::TarXz),
        (".tar", Format::Tar),
        (".zip", Format::Zip),
        (".rar", Format::Libarchive),
        (".7z", Format::Libarchive),
        (".gz", Format::Gz),
        (".bz2", Format::Bz2),
        (".xz", Format::Xz),
    ]
    .into_iter()
    .find(|(suffix, _)| name.ends_with(suffix))
    .map(|(_, f)| f)
    .ok_or_else(|| format!("unsupported archive format: {name}"))?;
    Ok(UnpackCommand {
        archive: archive.into(),
        destination: dest.into(),
        format,
    })
}

impl UnpackCommand {
    pub fn display(&self) -> String {
        format!(
            "Extract {} to {}",
            self.archive.display(),
            self.destination.display()
        )
    }

    pub async fn run(self) -> Result<(), String> {
        tokio::task::spawn_blocking(move || self.extract())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }

    fn extract(&self) -> io::Result<()> {
        let meta = self.archive.metadata()?;
        if !meta.is_file() {
            return Err(io::Error::other("Archive must be a regular file"));
        }
        let limit = meta
            .len()
            .saturating_mul(200)
            .clamp(EXPANSION_FLOOR, MAX_BYTES);
        let mut stage = Staged::new(limit)?;
        match self.format {
            Format::Tar => stage.tar(File::open(&self.archive)?)?,
            Format::TarGz => stage.tar(flate2::read::MultiGzDecoder::new(File::open(
                &self.archive,
            )?))?,
            Format::Zip => {
                if let Err(error) = stage.zip(&self.archive) {
                    if error.kind() != io::ErrorKind::Unsupported {
                        return Err(error);
                    }
                    // Use libarchive for ZIP compression methods outside our native reader.
                    stage = Staged::new(limit)?;
                    self.read_container(&mut stage)?;
                }
            }
            Format::TarXz | Format::TarBz2 => {
                let tool = if matches!(self.format, Format::TarXz) {
                    "xz"
                } else {
                    "bzip2"
                };
                if tool_available(tool) {
                    with_stdout(decoder_command(tool, &self.archive), |data| stage.tar(data))?;
                } else {
                    self.read_container(&mut stage)?;
                }
            }
            Format::Libarchive => self.read_container(&mut stage)?,
            Format::Gz | Format::Bz2 | Format::Xz => {
                let name = self
                    .archive
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|s| crate::util::valid_filename(s))
                    .ok_or_else(|| io::Error::other("Invalid output filename"))?;
                if matches!(self.format, Format::Gz) {
                    stage.file(
                        name,
                        None,
                        0,
                        flate2::read::MultiGzDecoder::new(File::open(&self.archive)?),
                    )?;
                } else {
                    let tool = if matches!(self.format, Format::Xz) {
                        "xz"
                    } else {
                        "bzip2"
                    };
                    let cmd = if tool_available(tool) {
                        decoder_command(tool, &self.archive)
                    } else {
                        let program = crate::util::find_sevenzip().ok_or_else(|| {
                            io::Error::other(format!(
                                "Install {tool} or 7z to decompress this file"
                            ))
                        })?;
                        let mut cmd = Command::new(program);
                        cmd.args(["x", "-so", "--"]).arg(&self.archive);
                        cmd
                    };
                    with_stdout(cmd, |data| stage.file(name, None, 0, data))?;
                }
            }
        }
        stage.publish(&self.destination)
    }

    // Convert to a TAR stream on stdout. The decoder never receives the output
    // directory, and its filenames go through the same validation as native TAR.
    fn read_container(&self, stage: &mut Staged) -> io::Result<()> {
        let mut archive_arg = std::ffi::OsString::from("@");
        archive_arg.push(self.archive.canonicalize()?);
        let mut cmd = Command::new(if cfg!(any(windows, target_os = "macos")) {
            "tar"
        } else {
            "bsdtar"
        });
        cmd.args(["-cPf", "-", "--format", "gnutar", "--"])
            .arg(archive_arg);
        with_stdout(cmd, |data| stage.tar(data)).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::other("Install bsdtar (libarchive-tools) to read this archive")
            } else {
                e
            }
        })
    }
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn decoder_command(tool: &str, archive: &Path) -> Command {
    let mut cmd = Command::new(tool);
    if tool == "xz" {
        cmd.arg("--memlimit-decompress=128MiB");
    }
    cmd.args(["-dc", "--"]).arg(archive);
    cmd
}

fn with_stdout<T>(
    mut cmd: Command,
    read: impl FnOnce(&mut std::process::ChildStdout) -> io::Result<T>,
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
