//! Unix-domain datagrams using the same generation-checked readiness driver.

use crate::{Result, SuspensionReason, net::io};
use std::{
    os::{fd::AsFd, unix::net::SocketAddr},
    path::Path,
};

/// An owned Unix datagram socket. Dropping a bound socket does not unlink its path.
#[derive(Debug)]
pub struct UnixDatagram {
    inner: std::os::unix::net::UnixDatagram,
}
impl UnixDatagram {
    /// Binds a filesystem path; the caller remains responsible for unlinking.
    pub fn bind(path: impl AsRef<Path>) -> Result<Self> {
        let inner = std::os::unix::net::UnixDatagram::bind(path.as_ref()).map_err(|error| {
            crate::Error::io("UnixDatagram bind", path.as_ref().display(), error)
        })?;
        io::checked(
            "set nonblocking",
            inner.as_fd(),
            inner.set_nonblocking(true),
        )?;
        Ok(Self { inner })
    }
    /// Creates an anonymous connected datagram pair.
    pub fn pair() -> Result<(Self, Self)> {
        let (left, right) = std::os::unix::net::UnixDatagram::pair()
            .map_err(|error| crate::Error::io("UnixDatagram pair", "anonymous endpoints", error))?;
        io::checked("set nonblocking", left.as_fd(), left.set_nonblocking(true))?;
        io::checked(
            "set nonblocking",
            right.as_fd(),
            right.set_nonblocking(true),
        )?;
        Ok((Self { inner: left }, Self { inner: right }))
    }
    /// Receives from the connected peer.
    pub fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || self.inner.recv(buffer),
        )
    }
    /// Sends to the connected peer.
    pub fn send(&self, buffer: &[u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || self.inner.send(buffer),
        )
    }
    /// Receives a datagram and its source address.
    pub fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || self.inner.recv_from(buffer),
        )
    }
    /// Sends a datagram to a filesystem socket path.
    pub fn send_to(&self, buffer: &[u8], path: impl AsRef<Path>) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || self.inner.send_to(buffer, path.as_ref()),
        )
    }
}

#[cfg(test)]
#[path = "datagram_test.rs"]
mod datagram_test;
