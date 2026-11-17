use std::{ffi::OsStr, fs, io, path::Path};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File, OpenOptions};

use super::{OperationEvents, copy_progress};

/// Stage data in the opened destination directory, then publish without
/// reopening that directory by its possibly replaced pathname.
pub(super) struct StagedCopy<'a> {
    parent: &'a Dir,
    name: String,
    file: File,
    published: bool,
    #[cfg(windows)]
    locked: Option<fs::File>,
}

impl<'a> StagedCopy<'a> {
    pub(super) fn new(parent: &'a Dir) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        for _ in 0..8 {
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
            let name = format!(".cpt-copy-{:032x}.part", u128::from_le_bytes(random));
            match parent.open_with(&name, &options) {
                Ok(file) => {
                    return Ok(Self {
                        parent,
                        name,
                        file,
                        published: false,
                        #[cfg(windows)]
                        locked: None,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("cannot allocate a temporary copy"))
    }

    pub(super) fn copy_from(
        &mut self,
        display: &Path,
        input: &fs::File,
        events: Option<&OperationEvents>,
    ) -> io::Result<()> {
        #[cfg(windows)]
        let output = {
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
            let mut options = OpenOptions::new();
            options
                .read(true)
                .write(true)
                .follow(FollowSymlinks::No)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            let locked = self.parent.open_with(&self.name, &options)?;
            if !super::same_file(&self.file.metadata()?, &locked.metadata()?) {
                return Err(io::Error::other("temporary copy changed"));
            }
            locked.into_std()
        };
        #[cfg(not(windows))]
        let output = self.file.try_clone()?.into_std();
        copy_progress::copy(display, input, &output, events)?;
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            // CopyFileEx does not share DELETE access. Acquire that access only
            // after copying, then verify the reopened object before publishing.
            drop(output);
            let mut options = OpenOptions::new();
            options
                .read(true)
                .follow(FollowSymlinks::No)
                .access_mode(DELETE | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            let locked = self.parent.open_with(&self.name, &options)?;
            if !super::same_file(&self.file.metadata()?, &locked.metadata()?) {
                return Err(io::Error::other(
                    "temporary copy changed before publication",
                ));
            }
            self.locked = Some(locked.into_std());
        }
        Ok(())
    }

    pub(super) fn persist(mut self, name: &OsStr, replace: bool) -> io::Result<()> {
        #[cfg(windows)]
        rename_open_file(
            self.locked.as_ref().expect("copy holds its file lock"),
            self.parent,
            name,
            replace,
        )?;
        #[cfg(not(windows))]
        if replace {
            self.parent.rename(&self.name, self.parent, name)?;
        } else {
            self.parent.hard_link(&self.name, self.parent, name)?;
            let _ = self.parent.remove_file(&self.name);
        }
        self.published = true;
        Ok(())
    }
}

impl Drop for StagedCopy<'_> {
    fn drop(&mut self) {
        if !self.published {
            #[cfg(windows)]
            {
                // A native copy can copy the read-only attribute too. Clear it
                // through our file handle before removing an unpublished file.
                if let Ok(meta) = self.file.metadata() {
                    let mut permissions = meta.permissions();
                    permissions.set_readonly(false);
                    let _ = self.file.set_permissions(permissions);
                }
                drop(self.locked.take());
            }
            let _ = self.parent.remove_file(&self.name);
        }
    }
}

#[cfg(windows)]
fn rename_open_file(file: &fs::File, parent: &Dir, name: &OsStr, replace: bool) -> io::Result<()> {
    use std::{
        mem::{offset_of, size_of},
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
        ptr,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };

    // The parent handle prevents Windows from renaming this directory while
    // the Win32 rename API consumes its absolute destination name.
    let mut destination = final_path(parent)?;
    destination.pop(); // terminating NUL
    if destination.last() != Some(&(b'\\' as u16)) {
        destination.push(b'\\' as u16);
    }
    destination.extend(name.encode_wide());
    let name = destination;
    let bytes = name
        .len()
        .checked_mul(2)
        .ok_or_else(|| io::Error::other("filename too long"))?;
    let length = u32::try_from(bytes).map_err(io::Error::other)?;
    let size = (offset_of!(FILE_RENAME_INFO, FileName) + bytes + size_of::<u16>())
        .max(size_of::<FILE_RENAME_INFO>());
    let size_u32 = u32::try_from(size).map_err(io::Error::other)?;
    // u64 storage supplies FILE_RENAME_INFO's alignment and enough initialized
    // trailing space for the UTF-16 name. Both handles remain live for the call.
    let mut buffer = vec![0u64; size.div_ceil(size_of::<u64>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    let ok = unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = ptr::null_mut();
        (*info).FileNameLength = length;
        ptr::copy_nonoverlapping(
            name.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfo, info.cast(), size_u32)
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn final_path(file: &impl std::os::windows::io::AsRawHandle) -> io::Result<Vec<u16>> {
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let mut path = vec![0; 1024];
    loop {
        // The writable UTF-16 buffer and file handle are valid throughout.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                path.as_mut_ptr(),
                path.len() as u32,
                0,
            )
        } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < path.len() {
            path.truncate(length + 1);
            return Ok(path);
        }
        path.resize(length + 1, 0);
    }
}
