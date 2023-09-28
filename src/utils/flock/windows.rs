#![allow(unsafe_code)]

//! Support for windows file locking. This implementation mainly pulls from
//! <https://learn.microsoft.com/en-us/windows/win32/fileio/locking-and-unlocking-byte-ranges-in-files>
//! in addition to cargo

use super::LockState;
use std::{fs::File, io::Error, os::windows::io::AsRawHandle, time::Duration};

type Result = std::io::Result<()>;

#[path = "win_bindings.rs"]
mod bindings;

use bindings::*;

macro_rules! flock_flag {
    ($state:expr) => {
        match $state {
            LockState::Shared => 0,
            LockState::Exclusive => LockFileFlags::LockfileExclusiveLock,
            _ => unreachable!(),
        }
    };
}

#[inline]
pub(super) fn try_lock(file: &File, state: LockState) -> Result {
    flock(
        file,
        flock_flag!(state) | LockFileFlags::LockfileFailImmediately,
        None,
    )
}

#[inline]
pub(super) fn lock(file: &File, state: LockState, timeout: Option<Duration>) -> Result {
    flock(file, flock_flag!(state), timeout)
}

fn flock(file: &File, flags: u32, timeout: Option<Duration>) -> Result {
    unsafe {
        let mut overlapped: Overlapped = std::mem::zeroed();
        overlapped.event = create_event_a(std::ptr::null(), 0, 0, std::ptr::null());

        if overlapped.event == 0 {
            return Err(Error::last_os_error());
        }

        let res = if lock_file_ex(
            file.as_raw_handle() as Handle,
            flags,
            0,
            !0,
            !0,
            &mut overlapped,
        ) == 0
        {
            let err = Error::last_os_error();

            if err
                .raw_os_error()
                .map_or(false, |x| x == Win32Error::ErrorIoPending as i32)
            {
                let mut locked = 0;
                let timeout = timeout.map_or(0, |dur| {
                    let millis = dur.as_millis();
                    if millis >= Infinite as u128 {
                        u32::MAX
                    } else {
                        millis as u32
                    }
                });

                if get_overlapped_result_ex(
                    file.as_raw_handle() as Handle,
                    &overlapped,
                    &mut locked,
                    timeout,
                    1,
                ) == 0
                {
                    Err(Error::last_os_error())
                } else {
                    Ok(())
                }
            } else {
                Err(err)
            }
        } else {
            Ok(())
        };

        close_handle(overlapped.event);
        res
    }
}

pub(super) fn unlock(file: &File) -> Result {
    unsafe {
        let ret = unlock_file(file.as_raw_handle() as Handle, 0, 0, !0, !0);
        if ret == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[inline]
pub(super) fn is_contended(err: &Error) -> bool {
    err.raw_os_error()
        .map_or(false, |x| x == Win32Error::ErrorLockViolation as i32)
}

#[inline]
pub(super) fn is_unsupported(err: &Error) -> bool {
    err.raw_os_error()
        .map_or(false, |x| x == Win32Error::ErrorInvalidFunction as i32)
}

#[inline]
pub(super) fn is_timed_out(err: &Error) -> bool {
    err.raw_os_error().map_or(false, |x| {
        x == Win32Error::WaitTimeout as i32 || x == Win32Error::WaitIoCompletion as i32
    })
}
