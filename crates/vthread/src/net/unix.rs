//! Unix-domain streams and datagrams. Path connection is delegated to native workers;
//! accept and data transfer use nonblocking readiness. Socket path cleanup is explicit.

mod datagram;
pub use datagram::UnixDatagram;

use super::io;
use crate::{Result, SuspensionReason, blocking};
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::{fd::AsFd, unix::net::SocketAddr},
    path::Path,
};

/// An owned nonblocking Unix-domain listener. Dropping it does not unlink its path.
#[derive(Debug)]
pub struct UnixListener {
    inner: std::os::unix::net::UnixListener,
}
/// An owned nonblocking Unix-domain stream.
#[derive(Debug)]
pub struct UnixStream {
    inner: std::os::unix::net::UnixStream,
}

impl UnixListener {
    /// Binds a filesystem socket path. The caller owns subsequent unlinking.
    pub fn bind(path: impl AsRef<Path>) -> Result<Self> {
        let inner = std::os::unix::net::UnixListener::bind(path)?;
        inner.set_nonblocking(true)?;
        Ok(Self { inner })
    }
    /// Accepts a connection using virtual read readiness.
    pub fn accept(&self) -> Result<(UnixStream, SocketAddr)> {
        let (inner, address) = io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoAccept,
            || self.inner.accept(),
        )?;
        inner.set_nonblocking(true)?;
        Ok((UnixStream { inner }, address))
    }
}
impl UnixStream {
    /// Connects a filesystem path on the bounded native pool, then enables nonblocking I/O.
    /// Cancellation cannot abort an already-running native connect attempt.
    pub fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        blocking::run_for(SuspensionReason::IoConnect, move || {
            let inner = std::os::unix::net::UnixStream::connect(path)?;
            inner.set_nonblocking(true)?;
            Ok(Self { inner })
        })?
    }
    /// Creates an anonymous connected pair without waiting for another endpoint.
    pub fn pair() -> Result<(Self, Self)> {
        let (left, right) = std::os::unix::net::UnixStream::pair()?;
        left.set_nonblocking(true)?;
        right.set_nonblocking(true)?;
        Ok((Self { inner: left }, Self { inner: right }))
    }
    /// Receives bytes or parks for read readiness; zero means EOF.
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || (&self.inner).read(buffer),
        )
    }
    /// Sends bytes or parks for write readiness.
    pub fn write(&self, buffer: &[u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || (&self.inner).write(buffer),
        )
    }
    /// Fills a buffer; earlier partial reads remain committed after errors.
    pub fn read_exact(&self, buffer: &mut [u8]) -> Result<()> {
        io::read_exact(|part| self.read(part), buffer)
    }
    /// Sends a buffer; earlier partial writes remain committed after errors.
    pub fn write_all(&self, buffer: &[u8]) -> Result<()> {
        io::write_all(|part| self.write(part), buffer)
    }
    /// Shuts down the specified directions.
    pub fn shutdown(&self, how: Shutdown) -> Result<()> {
        Ok(self.inner.shutdown(how)?)
    }
}

#[cfg(test)]
#[path = "unix_test.rs"]
mod unix_test;
