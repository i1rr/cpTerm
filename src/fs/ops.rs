use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::action::Action;

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
        .filter(|(_, (_, target))| target.exists())
        .map(|(i, _)| i)
        .collect()
}

/// Generate a unique target name by appending (1), (2), etc.
pub fn unique_target(target: &Path) -> PathBuf {
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
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Rename conflicting targets to unique names.
pub fn rename_conflicts(pairs: &mut [(PathBuf, PathBuf)]) {
    for (_, target) in pairs.iter_mut() {
        if target.exists() {
            *target = unique_target(target);
        }
    }
}

/// Copy a list of (source, target) pairs.
pub async fn copy_entries(pairs: Vec<(PathBuf, PathBuf)>, tx: mpsc::UnboundedSender<Action>) {
    let total = pairs.len() as u64;
    let mut done = 0u64;

    for (src, target) in &pairs {
        let result = if src.is_dir() {
            copy_dir_recursive(src, target)
        } else {
            std::fs::copy(src, target).map(|_| ())
        };

        match result {
            Ok(()) => {
                done += 1;
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to copy {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!(
        "Copied {} item(s)",
        done
    )));
}

/// Move a list of (source, target) pairs.
pub async fn move_entries(pairs: Vec<(PathBuf, PathBuf)>, tx: mpsc::UnboundedSender<Action>) {
    let total = pairs.len() as u64;
    let mut done = 0u64;

    for (src, target) in &pairs {
        let result = std::fs::rename(src, target).or_else(|_| {
            // rename fails across drives, fall back to copy+delete
            if src.is_dir() {
                copy_dir_recursive(src, target)?;
                std::fs::remove_dir_all(src)
            } else {
                std::fs::copy(src, target)?;
                std::fs::remove_file(src)
            }
        });

        match result {
            Ok(()) => {
                done += 1;
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to move {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!("Moved {} item(s)", done)));
}

/// Delete a list of files/dirs.
pub async fn delete_entries(sources: Vec<PathBuf>, tx: mpsc::UnboundedSender<Action>) {
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
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to delete {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!(
        "Deleted {} item(s)",
        done
    )));
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
