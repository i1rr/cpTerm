use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use tokio::sync::mpsc;

use crate::action::Action;
mod copy_progress;
mod move_cleanup;
mod staged_copy;
use move_cleanup::MoveCleanup;

#[derive(Clone)]
pub struct OperationEvents {
    pub(crate) id: u64,
    tx: mpsc::UnboundedSender<Action>,
}

impl From<mpsc::UnboundedSender<Action>> for OperationEvents {
    fn from(tx: mpsc::UnboundedSender<Action>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            tx,
        }
    }
}

impl OperationEvents {
    fn progress(&self, done: u64, total: u64) {
        let _ = self.tx.send(Action::OperationProgress {
            id: self.id,
            done,
            total,
        });
    }

    fn complete(&self, msg: String) {
        let _ = self.tx.send(Action::OperationComplete(self.id, msg));
    }
    fn error(&self, msg: String) {
        let _ = self.tx.send(Action::OperationError(self.id, msg));
    }
}

/// Build (source, target) pairs for a copy/move into dest_dir.
pub fn build_pairs(sources: &[PathBuf], dest_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    sources
        .iter()
        .map(|s| {
            let target = dest_dir.join(s.file_name().unwrap_or_default());
            (s.clone(), target)
        })
        .collect()
}

/// Find which targets already exist.
pub fn find_conflicts(pairs: &[(PathBuf, PathBuf)]) -> Vec<usize> {
    pairs
        .iter()
        .enumerate()
        .filter(|(_, (_, target))| target.symlink_metadata().is_ok())
        .map(|(i, _)| i)
        .collect()
}

/// Snapshot only destinations that already conflict, including children of
/// directories the overwrite choice would merge.
pub fn copy_conflicts(pairs: &[(PathBuf, PathBuf)]) -> HashSet<PathBuf> {
    fn visit(src: &Path, dest: &Path, conflicts: &mut HashSet<PathBuf>) {
        let Ok(target) = dest.symlink_metadata() else {
            return;
        };
        conflicts.insert(dest.to_owned());
        if target.is_dir()
            && !linked(&target)
            && src
                .symlink_metadata()
                .is_ok_and(|m| m.is_dir() && !linked(&m))
            && let Ok(entries) = src.read_dir()
        {
            for entry in entries.flatten() {
                visit(&entry.path(), &dest.join(entry.file_name()), conflicts);
            }
        }
    }
    let mut conflicts = HashSet::new();
    for (src, dest) in pairs {
        visit(src, dest, &mut conflicts);
    }
    conflicts
}

/// Generate a unique target name by appending (1), (2), etc.
pub fn unique_target(target: &Path) -> PathBuf {
    unique_target_except(target, &std::collections::HashSet::new())
}

fn unique_target_except(target: &Path, reserved: &std::collections::HashSet<PathBuf>) -> PathBuf {
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = target.parent().unwrap_or(Path::new("."));

    for i in 1.. {
        let candidate = parent.join(format!("{} ({}){}", stem, i, ext));
        if !reserved.contains(&candidate) && candidate.symlink_metadata().is_err() {
            return candidate;
        }
    }
    unreachable!()
}

/// Rename conflicting targets to unique names.
pub fn rename_conflicts(pairs: &mut [(PathBuf, PathBuf)]) {
    let mut reserved = pairs.iter().map(|(_, t)| t.clone()).collect();
    for (_, target) in pairs.iter_mut() {
        if target.symlink_metadata().is_ok() {
            *target = unique_target_except(target, &reserved);
            reserved.insert(target.clone());
        }
    }
}

/// Copy a list of (source, target) pairs.
pub async fn copy_entries(pairs: Vec<(PathBuf, PathBuf)>, tx: impl Into<OperationEvents>) {
    copy_entries_with_overwrites(pairs, HashSet::new(), tx).await;
}

pub async fn copy_entries_with_overwrites(
    pairs: Vec<(PathBuf, PathBuf)>,
    overwrites: HashSet<PathBuf>,
    tx: impl Into<OperationEvents>,
) {
    let tx = tx.into();
    let errors = tx.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || copy_blocking(pairs, overwrites, tx)).await
    {
        errors.error(format!("copy worker failed: {e}"));
    }
}

fn copy_blocking(
    pairs: Vec<(PathBuf, PathBuf)>,
    overwrites: HashSet<PathBuf>,
    tx: OperationEvents,
) {
    let total = pairs.len() as u64;
    let mut done = 0u64;

    for (src, target) in &pairs {
        let result = copy_tree(
            src,
            target,
            &|path| overwrites.contains(path),
            &mut |_, _| Ok(()),
            Some(&tx),
        );

        match result {
            Ok(()) => {
                done += 1;
                tx.progress(done, total);
            }
            Err(e) => {
                tx.error(format!("Failed to copy {}: {}", src.display(), e));
                return;
            }
        }
    }

    tx.complete(format!("Copied {} item(s)", done));
}

/// Move a list of (source, target) pairs.
pub async fn move_entries(pairs: Vec<(PathBuf, PathBuf)>, tx: impl Into<OperationEvents>) {
    let tx = tx.into();
    let errors = tx.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || move_blocking(pairs, tx)).await {
        errors.error(format!("move worker failed: {e}"));
    }
}

fn move_blocking(pairs: Vec<(PathBuf, PathBuf)>, tx: OperationEvents) {
    let total = pairs.len() as u64;
    let mut done = 0u64;

    for (src, target) in &pairs {
        let result = fs::rename(src, target).or_else(|e| {
            if e.kind() != io::ErrorKind::CrossesDevices {
                return Err(e);
            }
            MoveCleanup::copy(src, target)?.finish().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "copied to {}, source cleanup incomplete: {e}",
                        target.display()
                    ),
                )
            })
        });

        match result {
            Ok(()) => {
                done += 1;
                tx.progress(done, total);
            }
            Err(e) => {
                tx.error(format!("Failed to move {}: {}", src.display(), e));
                return;
            }
        }
    }

    tx.complete(format!("Moved {} item(s)", done));
}

/// Delete a list of files/dirs.
pub async fn delete_entries(sources: Vec<PathBuf>, tx: impl Into<OperationEvents>) {
    let tx = tx.into();
    let errors = tx.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || delete_blocking(sources, tx)).await {
        errors.error(format!("delete worker failed: {e}"));
    }
}

fn delete_blocking(sources: Vec<PathBuf>, tx: OperationEvents) {
    let total = sources.len() as u64;
    let mut done = 0u64;

    for src in &sources {
        let result = if src.is_dir() {
            std::fs::remove_dir_all(src)
        } else {
            std::fs::remove_file(src)
        };

        match result {
            Ok(()) => {
                done += 1;
                tx.progress(done, total);
            }
            Err(e) => {
                tx.error(format!("Failed to delete {}: {}", src.display(), e));
                return;
            }
        }
    }

    tx.complete(format!("Deleted {} item(s)", done));
}

fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    meta.is_symlink()
}

fn copy_tree(
    src: &Path,
    dest: &Path,
    can_replace: &dyn Fn(&Path) -> bool,
    inspect_source: &mut dyn FnMut(&Path, bool) -> io::Result<()>,
    events: Option<&OperationEvents>,
) -> io::Result<()> {
    let source = fs::canonicalize(src)?;
    let parent = fs::canonicalize(
        dest.parent()
            .ok_or_else(|| io::Error::other("missing destination parent"))?,
    )?;
    let target = parent.join(
        dest.file_name()
            .ok_or_else(|| io::Error::other("invalid destination"))?,
    );
    if target.starts_with(&source) || fs::canonicalize(dest).is_ok_and(|p| p.starts_with(&source)) {
        return Err(io::Error::other(
            "cannot copy an item onto or inside itself",
        ));
    }
    let source_parent = Dir::open_ambient_dir(
        src.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
        ambient_authority(),
    )?;
    let destination_parent = Dir::open_ambient_dir(parent, ambient_authority())?;
    CopyWalker {
        can_replace,
        inspect_source,
        events,
    }
    .copy_item(src, dest, &source_parent, &destination_parent)
}

fn same_file(a: &cap_std::fs::Metadata, b: &cap_std::fs::Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

fn capability_link(meta: &cap_std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    meta.is_symlink()
}

struct CopyWalker<'a> {
    can_replace: &'a dyn Fn(&Path) -> bool,
    inspect_source: &'a mut dyn FnMut(&Path, bool) -> io::Result<()>,
    events: Option<&'a OperationEvents>,
}

impl CopyWalker<'_> {
    fn copy_item(
        &mut self,
        src: &Path,
        dest: &Path,
        source_parent: &Dir,
        destination_parent: &Dir,
    ) -> io::Result<()> {
        let source_name = src
            .file_name()
            .ok_or_else(|| io::Error::other("invalid source"))?;
        let destination_name = dest
            .file_name()
            .ok_or_else(|| io::Error::other("invalid destination"))?;
        let meta = source_parent.symlink_metadata(source_name)?;
        if capability_link(&meta)
            || destination_parent
                .symlink_metadata(destination_name)
                .is_ok_and(|m| capability_link(&m))
        {
            return Err(io::Error::other("refusing to follow a link during copy"));
        }
        (self.inspect_source)(src, meta.is_dir())?;
        if meta.is_dir() {
            let source_dir = source_parent.open_dir_nofollow(source_name)?;
            if !same_file(&meta, &source_dir.dir_metadata()?) {
                return Err(io::Error::other("source directory changed during copy"));
            }
            match destination_parent.create_dir(destination_name) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && (self.can_replace)(dest) => {}
                result => result?,
            }
            // Every child operation stays relative to these open directories.
            // A replaced pathname cannot redirect a later write through a link.
            let destination_dir = destination_parent.open_dir_nofollow(destination_name)?;
            for entry in source_dir.entries()? {
                let name = entry?.file_name();
                self.copy_item(
                    &src.join(&name),
                    &dest.join(&name),
                    &source_dir,
                    &destination_dir,
                )?;
            }
        } else if meta.is_file() {
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_fs_ext::OpenOptionsSyncExt;
                // A regular source can be exchanged for a FIFO after metadata
                // inspection. Open without blocking, then validate the handle.
                options.nonblock(true);
            }
            #[cfg(windows)]
            {
                use cap_std::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
                // The native copier resolves the open file's name. Keep that
                // name fixed until it finishes, including all hard-link aliases.
                options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            }
            let input = source_parent.open_with(source_name, &options)?;
            let opened = input.metadata()?;
            if !opened.is_file() || capability_link(&opened) || !same_file(&meta, &opened) {
                return Err(io::Error::other("source file changed during copy"));
            }
            let input = input.into_std();
            let mut staged = staged_copy::StagedCopy::new(destination_parent)?;
            staged.copy_from(
                src,
                &input,
                self.events
                    .filter(|_| opened.len() >= copy_progress::MIN_SIZE),
            )?;
            // Release the Windows source lock before replacing a hard-link alias.
            drop(input);
            staged.persist(destination_name, (self.can_replace)(dest))?;
        } else {
            return Err(io::Error::other("cannot copy a special file"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod copy_race_tests {
    use super::*;

    fn directory_link(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Junctions need no symlink privilege. Pass paths as environment data,
            // never as PowerShell source, and create links only in this test tree.
            let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            assert!(
                std::process::Command::new(powershell)
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "New-Item -ItemType Junction -Path $env:CPT_TEST_LINK -Target $env:CPT_TEST_TARGET | Out-Null",
                    ])
                    .env("CPT_TEST_LINK", link)
                    .env("CPT_TEST_TARGET", target)
                    .creation_flags(0x08000000) // CREATE_NO_WINDOW
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }

    #[test]
    fn rejects_a_directory_link_inserted_after_the_initial_check() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let dest = root.path().join("dest");
        let moved = root.path().join("moved");
        let outside = root.path().join("outside");
        for path in [&src, &dest, &outside] {
            fs::create_dir(path).unwrap();
        }
        fs::write(src.join("payload"), b"copied data").unwrap();
        fs::write(outside.join("sentinel"), b"keep me").unwrap();
        let result = copy_tree(
            &src,
            &dest,
            &|_| true,
            &mut |path, _| {
                if path == src {
                    fs::rename(&dest, &moved)?;
                    directory_link(&outside, &dest);
                }
                Ok(())
            },
            None,
        );
        assert!(result.is_err());
        assert!(!outside.join("payload").exists());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep me");
    }

    #[test]
    fn an_open_copy_directory_cannot_redirect_a_child_write() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let dest = root.path().join("dest");
        let moved = root.path().join("moved");
        let outside = root.path().join("outside");
        for path in [&src, &dest, &outside] {
            fs::create_dir(path).unwrap();
        }
        let source_file = src.join("payload");
        fs::write(&source_file, b"copied data").unwrap();
        copy_tree(
            &src,
            &dest,
            &|_| true,
            &mut |path, _| {
                if path == source_file && fs::rename(&dest, &moved).is_ok() {
                    // Unix permits renaming an open directory. Windows directory
                    // handles prevent this rename, which is also a safe outcome.
                    directory_link(&outside, &dest);
                }
                Ok(())
            },
            None,
        )
        .unwrap();
        assert!(!outside.join("payload").exists());
        let copied = if moved.exists() { moved } else { dest };
        assert_eq!(fs::read(copied.join("payload")).unwrap(), b"copied data");
    }

    #[test]
    fn rejects_a_replaced_source_file_before_reading_its_contents() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("source");
        let saved = root.path().join("saved");
        let outside = root.path().join("outside");
        let dest = root.path().join("copy");
        fs::write(&src, b"original").unwrap();
        fs::write(&outside, b"do not copy this").unwrap();
        let result = copy_tree(
            &src,
            &dest,
            &|_| false,
            &mut |_, _| {
                fs::rename(&src, &saved)?;
                fs::hard_link(&outside, &src)
            },
            None,
        );
        assert!(result.is_err());
        assert!(!dest.exists());
        assert_eq!(fs::read(saved).unwrap(), b"original");
        assert_eq!(fs::read(outside).unwrap(), b"do not copy this");
    }

    #[cfg(unix)]
    #[test]
    fn a_source_replaced_by_a_fifo_cannot_block_the_copy() {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let root = tempfile::tempdir().unwrap();
            let src = root.path().join("source");
            let dest = root.path().join("copy");
            fs::write(&src, b"regular file").unwrap();
            let result = copy_tree(
                &src,
                &dest,
                &|_| false,
                &mut |path, _| {
                    fs::remove_file(path)?;
                    assert!(
                        std::process::Command::new("mkfifo")
                            .arg(path)
                            .status()?
                            .success()
                    );
                    Ok(())
                },
                None,
            );
            tx.send(result.is_err() && !dest.exists()).unwrap();
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("copy blocked on a FIFO")
        );
        worker.join().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn preserves_windows_streams_attributes_and_no_clobber_cleanup() {
        let root = tempfile::tempdir().unwrap();
        for size in [32, copy_progress::MIN_SIZE as usize] {
            let src = root.path().join(format!("source-{size}"));
            let dest = root.path().join(format!("copy-{size}"));
            let stream = |path: &Path| {
                let mut name = path.as_os_str().to_owned();
                name.push(":Zone.Identifier");
                PathBuf::from(name)
            };
            fs::write(&src, vec![b'x'; size]).unwrap();
            fs::write(stream(&src), b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
            let writable_permissions = src.metadata().unwrap().permissions();
            let mut permissions = writable_permissions.clone();
            permissions.set_readonly(true);
            fs::set_permissions(&src, permissions).unwrap();
            let (tx, _rx) = mpsc::unbounded_channel();
            let events = tx.into();
            copy_tree(&src, &dest, &|_| false, &mut |_, _| Ok(()), Some(&events)).unwrap();
            assert_eq!(fs::read(&dest).unwrap(), vec![b'x'; size]);
            assert_eq!(
                fs::read(stream(&dest)).unwrap(),
                fs::read(stream(&src)).unwrap()
            );
            assert!(dest.metadata().unwrap().permissions().readonly());
            assert!(copy_tree(&src, &dest, &|_| false, &mut |_, _| Ok(()), None).is_err());
            assert!(!root.path().read_dir().unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".cpt-copy-")
            }));
            for file in [&src, &dest] {
                fs::set_permissions(file, writable_permissions.clone()).unwrap();
            }
        }
    }
}
