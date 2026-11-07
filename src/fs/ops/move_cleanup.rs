use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};

/// A cross-volume move may remove only the source entries observed during a
/// successful copy. Never enumerate the source again to decide what to delete.
pub(super) struct MoveCleanup {
    base: PathBuf,
    parent: Dir,
    entries: Vec<(PathBuf, Stamp)>,
}

#[derive(PartialEq)]
struct Stamp {
    device: u64,
    inode: u64,
    file: Option<(u64, SystemTime)>,
}

impl MoveCleanup {
    pub(super) fn copy(src: &Path, dest: &Path) -> io::Result<Self> {
        let base = src
            .parent()
            .ok_or_else(|| io::Error::other("missing source parent"))?;
        let mut plan = Self {
            base: base.to_owned(),
            parent: Dir::open_ambient_dir(
                if base.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    base
                },
                ambient_authority(),
            )?,
            entries: Vec::new(),
        };
        super::copy_tree(
            src,
            dest,
            &|_| true,
            &mut |src, is_dir| plan.record(src, is_dir),
            None,
        )?;
        Ok(plan)
    }

    fn record(&mut self, src: &Path, is_dir: bool) -> io::Result<()> {
        let path = src.strip_prefix(&self.base).map_err(io::Error::other)?;
        let (parent, name) = entry_parent(&self.parent, path)?;
        self.entries
            .push((path.to_owned(), stamp(&parent, name, is_dir)?));
        Ok(())
    }

    pub(super) fn finish(self) -> io::Result<()> {
        let mut first_error = None;
        for (path, original) in self.entries.into_iter().rev() {
            let remove = || {
                let (parent, name) = entry_parent(&self.parent, &path)?;
                let is_dir = original.file.is_none();
                if stamp(&parent, name, is_dir)? != original {
                    return Err(io::Error::other("source entry changed after copy"));
                }
                if is_dir {
                    parent.remove_dir(name)
                } else {
                    parent.remove_file(name)
                }
            };
            if let Err(e) = remove() {
                first_error.get_or_insert_with(|| {
                    io::Error::new(e.kind(), format!("{}: {e}", self.base.join(path).display()))
                });
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn entry_parent<'a>(base: &Dir, path: &'a Path) -> io::Result<(Dir, &'a OsStr)> {
    let mut dir = base.try_clone()?;
    for part in path
        .parent()
        .ok_or_else(|| io::Error::other("missing entry parent"))?
        .iter()
    {
        dir = dir.open_dir_nofollow(part)?;
    }
    Ok((
        dir,
        path.file_name()
            .ok_or_else(|| io::Error::other("missing filename"))?,
    ))
}

fn stamp(parent: &Dir, name: &OsStr, is_dir: bool) -> io::Result<Stamp> {
    let meta = if is_dir {
        parent.open_dir_nofollow(name)?.dir_metadata()?
    } else {
        if !parent.symlink_metadata(name)?.is_file() {
            return Err(io::Error::other("source is no longer a regular file"));
        }
        parent
            .open_with(
                name,
                OpenOptions::new().read(true).follow(FollowSymlinks::No),
            )?
            .metadata()?
    };
    Ok(Stamp {
        device: meta.dev(),
        inode: meta.ino(),
        file: if is_dir {
            None
        } else {
            Some((meta.len(), meta.modified()?.into_std()))
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cross_device_cleanup_preserves_arrivals_and_changed_files() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let dest = root.path().join("dest");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("copied"), "original").unwrap();
        fs::write(src.join("edited"), "before").unwrap();
        let plan = MoveCleanup::copy(&src, &dest).unwrap();
        fs::write(src.join("arrived"), "new").unwrap();
        fs::write(src.join("edited"), "after copy").unwrap();
        assert!(plan.finish().is_err());
        assert_eq!(fs::read(src.join("arrived")).unwrap(), b"new");
        assert_eq!(fs::read(src.join("edited")).unwrap(), b"after copy");
        assert!(!src.join("copied").exists());
        assert_eq!(fs::read(dest.join("copied")).unwrap(), b"original");
        assert_eq!(fs::read(dest.join("edited")).unwrap(), b"before");
    }
}
