//! Retry advisory readiness with fresh, exact-generation registrations.

use crate::{Error, Parker, Result, SuspensionReason, context, sync::wait::Wait, wait::WaitCell};
use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd},
};

pub(crate) fn checked<T>(
    operation: &'static str,
    fd: BorrowedFd<'_>,
    result: io::Result<T>,
) -> Result<T> {
    result.map_err(|error| Error::io(operation, format_args!("fd={}", fd.as_raw_fd()), error))
}

pub(super) fn operation<T>(
    fd: BorrowedFd<'_>,
    interest: zio::Interest,
    reason: SuspensionReason,
    mut operation: impl FnMut() -> io::Result<T>,
) -> Result<T> {
    let _reason = Wait::enter(reason)?;
    loop {
        crate::checkpoint()?;
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => wait(fd, interest)?,
            Err(error) => {
                return Err(Error::io(
                    match reason {
                        SuspensionReason::IoRead => "socket read",
                        SuspensionReason::IoWrite => "socket write",
                        SuspensionReason::IoAccept => "socket accept",
                        _ => "socket operation",
                    },
                    format_args!("fd={}", fd.as_raw_fd()),
                    error,
                ));
            }
        }
    }
}

pub(super) fn wait(fd: BorrowedFd<'_>, interest: zio::Interest) -> Result<()> {
    let mounted = context::current().ok_or(Error::OutsideVThread)?;
    let execution = mounted.execution()?;
    let services = execution
        .shared
        .services
        .get()
        .ok_or(Error::RuntimeStopped)?;
    let parker = Parker {
        wait: WaitCell::new(),
    };
    parker.park_registered(|token, wake| services.reactor.register(fd, interest, token, wake))?;
    execution.data.check()?;
    services.reactor.check()
}

pub(super) fn read_exact(
    mut read: impl FnMut(&mut [u8]) -> Result<usize>,
    mut buffer: &mut [u8],
) -> Result<()> {
    crate::checkpoint()?;
    while !buffer.is_empty() {
        let count = read(buffer)?;
        if count == 0 {
            return Err(Error::io(
                "socket read_exact",
                "stream EOF",
                io::Error::from(io::ErrorKind::UnexpectedEof),
            ));
        }
        buffer = &mut buffer[count..];
    }
    Ok(())
}

pub(super) fn write_all(
    mut write: impl FnMut(&[u8]) -> Result<usize>,
    mut buffer: &[u8],
) -> Result<()> {
    crate::checkpoint()?;
    while !buffer.is_empty() {
        let count = write(buffer)?;
        if count == 0 {
            return Err(Error::io(
                "socket write_all",
                "stream made no progress",
                io::Error::from(io::ErrorKind::WriteZero),
            ));
        }
        buffer = &buffer[count..];
    }
    Ok(())
}

#[cfg(test)]
#[path = "io_test.rs"]
mod io_test;
