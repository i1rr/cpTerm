use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use super::OperationEvents;
use crate::action::{Action, CopyBytes};

pub(super) const MIN_SIZE: u64 = 1024 * 1024;

struct Reporter<'a> {
    events: &'a OperationEvents,
    bytes: CopyBytes,
    last_sent: Option<Instant>,
}

impl Reporter<'_> {
    fn send(&self) {
        let _ = self.events.tx.send(Action::CopyFileProgress {
            id: self.events.id,
            current: Some(self.bytes.clone()),
        });
    }

    fn update(&mut self, copied: u64, total: u64) {
        if copied <= self.bytes.copied {
            return;
        }
        self.bytes.copied = copied;
        self.bytes.total = total.max(copied);
        if copied == total
            || self
                .last_sent
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(100))
        {
            self.send();
            self.last_sent = Some(Instant::now());
        }
    }
}

pub(super) fn copy(
    src: &Path,
    tmp: &NamedTempFile,
    total: u64,
    events: &OperationEvents,
) -> io::Result<()> {
    let name = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace(char::is_control, "?");
    let mut reporter = Reporter {
        events,
        bytes: CopyBytes {
            name,
            copied: 0,
            total,
        },
        last_sent: None,
    };
    reporter.send();
    transfer(src, tmp, &mut reporter)?;
    let _ = events.tx.send(Action::CopyFileProgress {
        id: events.id,
        current: None,
    });
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn transfer(src: &Path, tmp: &NamedTempFile, progress: &mut Reporter<'_>) -> io::Result<()> {
    use std::{fs::File, io::Read};
    let mut input = File::open(src)?;
    let meta = input.metadata()?;
    let mut written = 0;
    loop {
        let n = io::copy(&mut (&mut input).take(256 * 1024), &mut tmp.as_file())?;
        if n == 0 {
            break;
        }
        written += n;
        progress.update(written, meta.len());
    }
    tmp.as_file().set_permissions(meta.permissions())
}

#[cfg(windows)]
fn transfer(src: &Path, tmp: &NamedTempFile, progress: &mut Reporter<'_>) -> io::Result<()> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{CopyFileExW, PROGRESS_CONTINUE},
    };
    unsafe extern "system" fn report(
        total: i64,
        copied: i64,
        _: i64,
        _: i64,
        _: u32,
        _: u32,
        _: HANDLE,
        _: HANDLE,
        ctx: *const c_void,
    ) -> u32 {
        // CopyFileExW calls back synchronously with the live Reporter supplied below.
        let progress = unsafe { &mut *ctx.cast_mut().cast::<Reporter<'_>>() };
        progress.update(copied as u64, total as u64);
        PROGRESS_CONTINUE
    }
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        Ok(path
            .canonicalize()?
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect())
    }
    let (src, dest) = (wide(src)?, wide(tmp.path())?);
    // Use the native copier to preserve alternate data streams and file attributes.
    let ok = unsafe {
        CopyFileExW(
            src.as_ptr(),
            dest.as_ptr(),
            Some(report),
            ptr::from_mut(progress).cast(),
            ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn transfer(src: &Path, tmp: &NamedTempFile, progress: &mut Reporter<'_>) -> io::Result<()> {
    use std::{
        ffi::{c_char, c_void},
        fs::File,
        os::fd::AsRawFd,
        ptr,
    };
    unsafe extern "C" fn report(
        what: i32,
        stage: i32,
        state: libc::copyfile_state_t,
        _: *const c_char,
        _: *const c_char,
        ctx: *mut c_void,
    ) -> i32 {
        // CONTINUE on a write error asks fcopyfile to retry, even for a full disk.
        if stage == libc::COPYFILE_ERR {
            return libc::COPYFILE_QUIT;
        }
        if what == libc::COPYFILE_COPY_DATA && stage == libc::COPYFILE_PROGRESS {
            // COPYFILE_STATE_COPIED writes an off_t. The callback context stays live for fcopyfile.
            let mut copied: libc::off_t = 0;
            if unsafe {
                libc::copyfile_state_get(
                    state,
                    libc::COPYFILE_STATE_COPIED as u32,
                    ptr::from_mut(&mut copied).cast(),
                )
            } == 0
            {
                let progress = unsafe { &mut *ctx.cast::<Reporter<'_>>() };
                progress.update(copied as u64, progress.bytes.total);
            }
        }
        libc::COPYFILE_CONTINUE
    }
    let input = File::open(src)?;
    // fcopyfile preserves ACLs, resource forks and extended attributes. All handles
    // and the callback context outlive the synchronous call.
    unsafe {
        let state = libc::copyfile_state_alloc();
        if state.is_null() {
            return Err(io::Error::last_os_error());
        }
        let ok = libc::copyfile_state_set(
            state,
            libc::COPYFILE_STATE_STATUS_CB as u32,
            report as *const c_void,
        ) == 0
            && libc::copyfile_state_set(
                state,
                libc::COPYFILE_STATE_STATUS_CTX as u32,
                ptr::from_mut(progress).cast(),
            ) == 0
            && libc::fcopyfile(
                input.as_raw_fd(),
                tmp.as_file().as_raw_fd(),
                state,
                libc::COPYFILE_DATA | libc::COPYFILE_METADATA,
            ) == 0;
        let result = if ok {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        };
        libc::copyfile_state_free(state);
        result
    }
}
