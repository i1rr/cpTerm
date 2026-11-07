use std::fs;
use std::io::{self, Write};
use std::path::Path;

use cpt::fs::archive::resolve_unpack_command;
use flate2::{Compression, write::GzEncoder};
use zip::{ZipWriter, write::SimpleFileOptions};

// Set the raw header name so the fixture writer cannot sanitize the attack.
fn tar_member<W: Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    kind: tar::EntryType,
    target: Option<&str>,
    size: u64,
) {
    use std::io::Read;
    let mut header = tar::Header::new_gnu();
    header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
    header.set_entry_type(kind);
    header.set_mode(0o644);
    header.set_size(size);
    if let Some(target) = target {
        header.set_link_name_literal(target).unwrap();
    }
    header.set_cksum();
    tar.append(&header, io::repeat(0).take(size)).unwrap();
}

fn make_tar(path: &Path, name: &str, link: Option<tar::EntryType>) {
    let mut tar = tar::Builder::new(fs::File::create(path).unwrap());
    tar_member(&mut tar, "first", tar::EntryType::Regular, None, 1);
    if let Some(kind) = link {
        tar_member(&mut tar, name, kind, Some("../outside"), 0);
        tar_member(&mut tar, "pivot/injected", tar::EntryType::Regular, None, 1);
    } else {
        tar_member(&mut tar, name, tar::EntryType::Regular, None, 1);
    }
    tar.finish().unwrap();
}

fn make_zip(path: &Path, name: &str, link: bool) {
    let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("first", options).unwrap();
    zip.write_all(b"ok").unwrap();
    if link {
        zip.add_symlink(name, "../outside", options).unwrap();
        zip.start_file("pivot/injected", options).unwrap();
    } else {
        zip.start_file(name, options).unwrap();
    }
    zip.write_all(b"injected").unwrap();
    zip.finish().unwrap();
}

struct Fixture {
    _root: tempfile::TempDir,
    dest: std::path::PathBuf,
    outside: std::path::PathBuf,
    archive: std::path::PathBuf,
}

impl Fixture {
    fn new(ext: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("out");
        let outside = root.path().join("outside");
        fs::create_dir(&dest).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"original").unwrap();
        let archive = root.path().join(format!("fixture.{ext}"));
        Self {
            _root: root,
            dest,
            outside,
            archive,
        }
    }

    async fn rejected(&self, reason: &str) {
        let error = unpack(&self.archive, &self.dest)
            .await
            .expect_err("unsafe archive succeeded");
        assert!(error.contains(reason), "expected {reason:?}, got {error:?}");
        assert!(!self.dest.join("first").exists());
        assert_eq!(
            fs::read(self.outside.join("sentinel")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read_dir(&self.outside).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn tar_and_zip_reject_escape_paths_before_publishing_any_member() {
    for ext in ["tar", "zip"] {
        let f = Fixture::new(ext);
        for name in [
            "../outside/sentinel",
            "safe/../../outside/injected",
            r"..\outside\injected",
            "/absolute-name",
            r"C:\outside\injected",
            r"\\host\share\file",
        ] {
            if ext == "tar" {
                make_tar(&f.archive, name, None);
            } else {
                make_zip(&f.archive, name, false);
            }
            f.rejected("path").await;
            assert_eq!(fs::read_dir(&f.dest).unwrap().count(), 0);
        }
    }
}

#[tokio::test]
async fn archived_symlinks_and_hard_links_never_reach_the_filesystem() {
    let f = Fixture::new("tar");
    for kind in [tar::EntryType::Symlink, tar::EntryType::Link] {
        make_tar(&f.archive, "pivot", Some(kind));
        f.rejected("link").await;
        assert!(f.dest.join("pivot").symlink_metadata().is_err());
    }
    let f = Fixture::new("zip");
    make_zip(&f.archive, "pivot", true);
    f.rejected("link").await;
    assert!(f.dest.join("pivot").symlink_metadata().is_err());
}

#[tokio::test]
async fn existing_destination_directory_link_cannot_redirect_extraction() {
    let f = Fixture::new("zip");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&f.outside, f.dest.join("pivot")).unwrap();
    #[cfg(windows)]
    assert!(
        std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J", "out\\pivot", "outside"])
            .current_dir(f.dest.parent().unwrap())
            .output()
            .unwrap()
            .status
            .success()
    );
    make_zip(&f.archive, "pivot/injected", false);
    f.rejected("").await;
    assert_eq!(fs::read_dir(&f.dest).unwrap().count(), 1);
}

#[tokio::test]
async fn zip_and_compressed_tar_share_one_expansion_budget_across_files() {
    use std::io::Read;
    let size = 5 * 1024 * 1024;
    for ext in ["zip", "tar.gz"] {
        let f = Fixture::new(ext);
        let file = fs::File::create(&f.archive).unwrap();
        if ext == "zip" {
            let mut zip = ZipWriter::new(file);
            for name in ["first", "second"] {
                zip.start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
                io::copy(&mut io::repeat(0).take(size), &mut zip).unwrap();
            }
            zip.finish().unwrap();
        } else {
            let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::fast()));
            for name in ["first", "second"] {
                tar_member(&mut tar, name, tar::EntryType::Regular, None, size);
            }
            tar.into_inner().unwrap().finish().unwrap();
        }
        assert!(fs::metadata(&f.archive).unwrap().len() * 200 < size * 2);
        f.rejected("byte limit").await;
        assert_eq!(fs::read_dir(&f.dest).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn tiny_members_and_oversized_tar_metadata_are_bounded() {
    let f = Fixture::new("tgz");
    let mut tar = tar::Builder::new(GzEncoder::new(
        fs::File::create(&f.archive).unwrap(),
        Compression::fast(),
    ));
    for i in 0..10_001 {
        tar_member(
            &mut tar,
            &format!("file{i}"),
            tar::EntryType::Regular,
            None,
            0,
        );
    }
    tar.into_inner().unwrap().finish().unwrap();
    f.rejected("entry limit").await;
    assert_eq!(fs::read_dir(&f.dest).unwrap().count(), 0);

    let mut tar = tar::Builder::new(GzEncoder::new(
        fs::File::create(&f.archive).unwrap(),
        Compression::fast(),
    ));
    tar_member(
        &mut tar,
        "longname",
        tar::EntryType::GNULongName,
        None,
        2 * 1024 * 1024,
    );
    tar.into_inner().unwrap().finish().unwrap();
    f.rejected("metadata limit").await;
}

#[tokio::test]
async fn normal_archive_merges_directories_and_preserves_existing_files() {
    let f = Fixture::new("tar");
    fs::create_dir(f.dest.join("sub")).unwrap();
    fs::write(f.dest.join("first"), b"original").unwrap();
    make_tar(&f.archive, "./sub/new", None);
    unpack(&f.archive, &f.dest).await.unwrap();
    assert_eq!(fs::read(f.dest.join("first")).unwrap(), b"original");
    assert_eq!(fs::read(f.dest.join("sub/new")).unwrap(), [0]);
}

#[tokio::test]
async fn seven_zip_decoder_feeds_the_same_path_checks() {
    let tool = if cfg!(any(windows, target_os = "macos")) {
        "tar"
    } else {
        "bsdtar"
    };
    // Optional on developer machines. CI provisions this tool on all platforms.
    if std::process::Command::new(tool)
        .arg("--version")
        .output()
        .is_err()
    {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must exercise the 7z decoder"
        );
        return;
    }
    for (name, safe) in [("safe/file", true), ("../outside/injected", false)] {
        let f = Fixture::new("7z");
        let source = f.archive.with_extension("tar");
        make_tar(&source, name, None);
        let result = std::process::Command::new(tool)
            .current_dir(source.parent().unwrap())
            .args(["-cPf", "fixture.7z", "--format", "7zip", "@fixture.tar"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if safe {
            unpack(&f.archive, &f.dest).await.unwrap();
            assert_eq!(fs::read(f.dest.join(name)).unwrap(), [0]);
        } else {
            f.rejected("path").await;
        }
    }
}

fn gzip_member(bytes: u64) -> Vec<u8> {
    use std::io::Read;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    io::copy(&mut io::repeat(0).take(bytes), &mut encoder).unwrap();
    encoder.finish().unwrap()
}

async fn unpack(archive: &Path, dest: &Path) -> Result<(), String> {
    resolve_unpack_command(archive, dest)?.run().await
}

#[tokio::test]
async fn gzip_expansion_limit_counts_concatenated_members_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("data.gz");
    let dest = root.path().join("out");
    fs::create_dir(&dest).unwrap();
    let limit = 8 * 1024 * 1024;
    let member = gzip_member(limit / 2);
    fs::write(&archive, [member.as_slice(), member.as_slice()].concat()).unwrap();
    assert!(fs::metadata(&archive).unwrap().len() * 200 < limit);
    unpack(&archive, &dest).await.unwrap();
    let output = dest.join("data");
    assert_eq!(fs::metadata(&output).unwrap().len(), limit);
    fs::remove_file(output).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&archive)
        .unwrap()
        .write_all(&gzip_member(1))
        .unwrap();
    assert!(
        unpack(&archive, &dest)
            .await
            .unwrap_err()
            .contains("byte limit")
    );
    assert_eq!(fs::read_dir(dest).unwrap().count(), 0);
}
