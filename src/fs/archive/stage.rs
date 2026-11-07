use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cap_fs_ext::DirExt;
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};

use super::MAX_ENTRIES;

enum Item {
    Directory(u32),
    File { offset: u64, size: u64, mode: u32 },
}

// Keep contents in one anonymous file. Names from an unvalidated archive
// never become filesystem paths. Publication starts only after decoding succeeds.
pub(super) struct Staged {
    data: File,
    items: BTreeMap<PathBuf, Item>,
    members: usize,
    bytes_left: u64,
}

fn member_path(name: &str) -> io::Result<PathBuf> {
    if name.starts_with(['/', '\\']) || name.len() > 4096 {
        return Err(io::Error::other("Unsafe archive member path"));
    }
    let mut path = PathBuf::new();
    for part in name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        if !crate::util::valid_filename(part) || part.contains(':') || part.len() > 255 {
            return Err(io::Error::other(format!(
                "Unsafe archive member path: {name:?}"
            )));
        }
        path.push(part);
    }
    if path.components().count() > 64 {
        return Err(io::Error::other("Archive path is too deep"));
    }
    Ok(path)
}

impl Staged {
    pub fn new(limit: u64) -> io::Result<Self> {
        Ok(Self {
            data: tempfile::tempfile()?,
            items: BTreeMap::new(),
            members: 0,
            bytes_left: limit,
        })
    }

    fn record_path(&mut self, name: &str, dir: bool) -> io::Result<PathBuf> {
        self.members += 1;
        if self.members > MAX_ENTRIES {
            return Err(io::Error::other("Archive exceeds the entry limit"));
        }
        let path = member_path(name)?;
        if !dir && path.as_os_str().is_empty() {
            return Err(io::Error::other("Empty archive filename"));
        }
        let dirs = if dir {
            Some(path.as_path())
        } else {
            path.parent()
        };
        for parent in dirs
            .into_iter()
            .flat_map(Path::ancestors)
            .filter(|p| !p.as_os_str().is_empty())
        {
            if !matches!(
                self.items
                    .entry(parent.into())
                    .or_insert(Item::Directory(0o777)),
                Item::Directory(_)
            ) {
                return Err(io::Error::other("Archive file conflicts with a directory"));
            }
        }
        if self.items.len() + usize::from(!dir) > MAX_ENTRIES {
            return Err(io::Error::other("Archive exceeds the entry limit"));
        }
        Ok(path)
    }

    fn directory(&mut self, name: &str, mode: u32) -> io::Result<()> {
        let path = self.record_path(name, true)?;
        if !path.as_os_str().is_empty() {
            self.items.insert(path, Item::Directory(mode));
        }
        Ok(())
    }

    pub fn file(
        &mut self,
        name: &str,
        size: Option<u64>,
        mode: u32,
        mut reader: impl Read,
    ) -> io::Result<()> {
        let path = self.record_path(name, false)?;
        if self.items.contains_key(&path) {
            return Err(io::Error::other("Conflicting archive entries"));
        }
        let limit = size.unwrap_or(self.bytes_left);
        if limit > self.bytes_left {
            return Err(io::Error::other("Archive exceeds the expanded byte limit"));
        }
        let offset = self.data.stream_position()?;
        let written = io::copy(&mut reader.by_ref().take(limit), &mut self.data)?;
        if reader.read(&mut [0])? != 0 {
            return Err(io::Error::other(
                "Archive exceeds the expanded byte limit or declared size",
            ));
        }
        if size.is_some_and(|s| s != written) {
            return Err(io::Error::other("Truncated archive member"));
        }
        self.bytes_left -= written;
        self.items.insert(
            path,
            Item::File {
                offset,
                size: written,
                mode,
            },
        );
        Ok(())
    }

    pub fn zip(&mut self, path: &Path) -> io::Result<()> {
        use zip::unstable::read::ZipIterable;
        let input = File::open(path)?;
        let body = input.try_clone()?;
        let mut archive =
            ZipIterable::try_new(input, Default::default())?.enforce_header_consistency(true);
        if archive.len() > MAX_ENTRIES {
            return Err(io::Error::other("Archive exceeds the entry limit"));
        }
        let mut files = archive.files_with_reader(body, Default::default())?;
        while let Some(entry) = files.next() {
            let entry = entry?;
            let name = entry.name()?.into_owned();
            let mode = entry.unix_mode().unwrap_or(0);
            if entry.is_symlink() || !matches!(mode & 0o170000, 0 | 0o040000 | 0o100000) {
                return Err(io::Error::other("Archive contains a link or special file"));
            }
            if entry.is_dir() {
                if entry.size() != 0 {
                    return Err(io::Error::other("Unexpected data in archive directory"));
                }
                self.directory(&name, entry.unix_mode().unwrap_or(0o777))?;
            } else {
                self.file(
                    &name,
                    Some(entry.size()),
                    entry.unix_mode().unwrap_or(0o666),
                    entry,
                )?;
            }
        }
        Ok(())
    }

    pub fn tar(&mut self, input: impl Read) -> io::Result<()> {
        // The TAR reader consumes GNU/PAX extension records before returning a
        // member. Bound that work separately from the body of a regular file.
        const HEADER_LIMIT: u64 = 1024 * 1024;
        let allowance = Cell::new(HEADER_LIMIT);
        let mut archive = tar::Archive::new(TarReader {
            input,
            allowance: &allowance,
            total: self.bytes_left + 16 * HEADER_LIMIT,
        });
        for entry in archive.entries()? {
            let entry = entry?;
            let kind = entry.header().entry_type();
            let name =
                String::from_utf8(entry.path_bytes().into_owned()).map_err(io::Error::other)?;
            if kind.is_dir() && entry.size() == 0 {
                self.directory(&name, entry.header().mode()?)?;
            } else if kind.is_file() {
                allowance.set(entry.size());
                let mode = entry.header().mode()?;
                self.file(&name, Some(entry.size()), mode, entry)?;
            } else {
                return Err(io::Error::other("Archive contains a link or special file"));
            }
            allowance.set(HEADER_LIMIT);
        }
        // Read through the compression trailer to check its integrity. Only TAR
        // block padding belongs after the end marker.
        let mut reader = archive.into_inner();
        let mut buf = [0; 8192];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if buf[..n].iter().any(|b| *b != 0) {
                return Err(io::Error::other("Unexpected data after the TAR end marker"));
            }
        }
        Ok(())
    }

    pub fn publish(mut self, destination: &Path) -> io::Result<()> {
        let root = open_destination(destination)?;
        // Inspect existing directories first, so a link later in the manifest
        // cannot leave earlier entries visible. Reopen without following links
        // during publication as well, including if another process changes them.
        for (path, item) in &self.items {
            if let Some(parent) = parent_dir(&root, path)? {
                check_existing(&parent, path.file_name().expect("validated path"), item)?;
            }
        }
        publish_directory(
            &root,
            Path::new(""),
            &mut self.items.iter().peekable(),
            &mut self.data,
        )
    }
}

// Parents sort before their children. Keep one open directory per level so a
// read-only directory can receive its final mode after its contents are written.
fn publish_directory(
    dir: &Dir,
    prefix: &Path,
    entries: &mut std::iter::Peekable<std::collections::btree_map::Iter<'_, PathBuf, Item>>,
    data: &mut File,
) -> io::Result<()> {
    while let Some((path, item)) = entries.next_if(|(path, _)| path.parent() == Some(prefix)) {
        let name = path.file_name().expect("validated path");
        let exists = check_existing(dir, name, item)?;
        match *item {
            Item::Directory(mode) => {
                if !exists {
                    create_directory(dir, name, mode)?;
                }
                let child = dir.open_dir_nofollow(name)?;
                let result = publish_directory(&child, path, entries, data);
                #[cfg(unix)]
                if !exists && mode & 0o700 != 0o700 {
                    use cap_std::fs::PermissionsExt;
                    let mut perm = child.dir_metadata()?.permissions();
                    perm.set_mode(perm.mode() & mode & 0o777);
                    child.set_permissions(".", perm)?;
                }
                result?;
            }
            Item::File { offset, size, mode } if !exists => {
                data.seek(SeekFrom::Start(offset))?;
                let mut out = dir.open_with(name, &file_options(mode))?;
                if let Err(e) = io::copy(&mut data.take(size), &mut out) {
                    drop(out);
                    let _ = dir.remove_file(name);
                    return Err(e);
                }
            }
            Item::File { .. } => {}
        }
    }
    Ok(())
}

fn create_directory(dir: &Dir, name: &std::ffi::OsStr, _mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use cap_std::fs::DirBuilderExt;
        let mut builder = cap_std::fs::DirBuilder::new();
        dir.create_dir_with(name, builder.mode((_mode & 0o777) | 0o700))
    }
    #[cfg(not(unix))]
    dir.create_dir(name)
}

fn file_options(_mode: u32) -> OpenOptions {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        opts.mode(_mode & 0o777);
    }
    opts
}

struct TarReader<'a, R> {
    input: R,
    allowance: &'a Cell<u64>,
    total: u64,
}
impl<R: Read> Read for TarReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let limit = self.allowance.get().min(self.total).min(buf.len() as u64) as usize;
        if limit == 0 && !buf.is_empty() {
            return Err(io::Error::other("Archive exceeds the TAR metadata limit"));
        }
        let n = self.input.read(&mut buf[..limit])?;
        self.allowance.set(self.allowance.get() - n as u64);
        self.total -= n as u64;
        Ok(n)
    }
}

fn open_destination(path: &Path) -> io::Result<Dir> {
    // A user-selected ancestor can be a mount point. Anchor all member paths
    // to the selected directory, and refuse a symlink as the root itself.
    if let Some(name) = path.file_name() {
        let parent = path.parent().unwrap_or(Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        std::fs::create_dir_all(parent)?;
        let parent = Dir::open_ambient_dir(parent, ambient_authority())?;
        match parent.create_dir(name) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            result => result?,
        }
        parent.open_dir_nofollow(name)
    } else {
        Dir::open_ambient_dir(path, ambient_authority())
    }
}

fn parent_dir(root: &Dir, path: &Path) -> io::Result<Option<Dir>> {
    let mut dir = root.try_clone()?;
    for part in path.parent().expect("validated path").iter() {
        dir = match dir.open_dir_nofollow(part) {
            Ok(dir) => dir,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    Ok(Some(dir))
}

fn check_existing(dir: &Dir, name: &std::ffi::OsStr, item: &Item) -> io::Result<bool> {
    let meta = match dir.symlink_metadata(name) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    match item {
        Item::Directory(_) if meta.is_dir() => {
            dir.open_dir_nofollow(name)?;
        }
        Item::File { .. } if meta.is_file() => {}
        _ => {
            return Err(io::Error::other(format!(
                "Unsafe or conflicting extraction target: {name:?}"
            )));
        }
    }
    Ok(true)
}
