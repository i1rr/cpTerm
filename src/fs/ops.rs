use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use tokio::sync::mpsc;

use crate::action::Action;
mod move_cleanup;
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
    copy_item(src, dest, can_replace, inspect_source)
}

fn copy_item(
    src: &Path,
    dest: &Path,
    can_replace: &dyn Fn(&Path) -> bool,
    inspect_source: &mut dyn FnMut(&Path, bool) -> io::Result<()>,
) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if linked(&meta) || dest.symlink_metadata().is_ok_and(|m| linked(&m)) {
        return Err(io::Error::other("refusing to follow a link during copy"));
    }
    inspect_source(src, meta.is_dir())?;
    if meta.is_dir() {
        match fs::create_dir(dest) {
            Err(e)
                if e.kind() == io::ErrorKind::AlreadyExists
                    && can_replace(dest)
                    && dest.is_dir() => {}
            result => result?,
        }
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_item(
                &entry.path(),
                &dest.join(entry.file_name()),
                can_replace,
                inspect_source,
            )?;
        }
    } else if meta.is_file() {
        // Publish only complete copies. Replacing a hard link does not truncate its source.
        let tmp = tempfile::NamedTempFile::new_in(
            dest.parent()
                .ok_or_else(|| io::Error::other("missing destination parent"))?,
        )?;
        fs::copy(src, tmp.path())?;
        if can_replace(dest) {
            tmp.persist(dest)?;
        } else {
            tmp.persist_noclobber(dest).map_err(|e| {
                io::Error::new(
                    e.error.kind(),
                    format!("Cannot create {}: {}", dest.display(), e.error),
                )
            })?;
        }
    } else {
        return Err(io::Error::other("cannot copy a special file"));
    }
    Ok(())
}
