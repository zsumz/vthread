//! TCP listener and stream operations with explicit virtual suspension.

use super::io;
use crate::{Result, SuspensionReason};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr},
    os::fd::AsFd,
};

/// An owned nonblocking TCP listener; accept parks only its virtual caller.
#[derive(Debug)]
pub struct TcpListener {
    inner: std::net::TcpListener,
}
/// An owned nonblocking TCP stream, shareable across virtual threads.
/// Concurrent readers/writers follow OS stream semantics; they are not message framing.
#[derive(Debug)]
pub struct TcpStream {
    pub(super) inner: std::net::TcpStream,
}

impl TcpListener {
    /// Binds a numeric address without hostname resolution.
    pub fn bind(address: SocketAddr) -> Result<Self> {
        let inner = std::net::TcpListener::bind(address)?;
        inner.set_nonblocking(true)?;
        Ok(Self { inner })
    }
    /// Returns the bound address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.local_addr()?)
    }
    /// Accepts a connection, parking for read readiness when none is queued.
    pub fn accept(&self) -> Result<(TcpStream, SocketAddr)> {
        let (stream, address) = io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoAccept,
            || self.inner.accept(),
        )?;
        stream.set_nonblocking(true)?;
        Ok((TcpStream { inner: stream }, address))
    }
}
impl TcpStream {
    /// Establishes a nonblocking TCP connection using writable readiness and SO_ERROR.
    pub fn connect(address: SocketAddr) -> Result<Self> {
        Ok(Self {
            inner: super::connect::tcp(address)?,
        })
    }
    /// Receives bytes, returning zero at EOF. Cancellation may follow earlier partial reads.
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || (&self.inner).read(buffer),
        )
    }
    /// Sends bytes; readiness does not imply the whole input fits.
    pub fn write(&self, buffer: &[u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || (&self.inner).write(buffer),
        )
    }
    /// Fills the buffer or fails. Bytes read before cancellation/error are not rolled back.
    pub fn read_exact(&self, buffer: &mut [u8]) -> Result<()> {
        io::read_exact(|part| self.read(part), buffer)
    }
    /// Sends all bytes or fails; earlier writes remain committed on cancellation/error.
    pub fn write_all(&self, buffer: &[u8]) -> Result<()> {
        io::write_all(|part| self.write(part), buffer)
    }
    /// Shuts down one or both directions, waking affected readiness waits through the OS.
    pub fn shutdown(&self, how: Shutdown) -> Result<()> {
        Ok(self.inner.shutdown(how)?)
    }
    /// Returns the local socket address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.local_addr()?)
    }
    /// Returns the peer socket address.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.peer_addr()?)
    }
    /// Controls TCP_NODELAY.
    pub fn set_nodelay(&self, enabled: bool) -> Result<()> {
        Ok(self.inner.set_nodelay(enabled)?)
    }
}

#[cfg(test)]
#[path = "tcp_test.rs"]
mod tcp_test;
