//! Nonblocking UDP with bounded readiness subscriptions.

use super::io;
use crate::{Result, SuspensionReason};
use std::{net::SocketAddr, os::fd::AsFd};

/// An owned UDP socket. Receive buffers may truncate a datagram as with std UDP.
#[derive(Debug)]
pub struct UdpSocket {
    inner: std::net::UdpSocket,
}
impl UdpSocket {
    /// Binds a numeric address without DNS resolution.
    pub fn bind(address: SocketAddr) -> Result<Self> {
        let inner = std::net::UdpSocket::bind(address)
            .map_err(|error| crate::Error::io("UDP bind", address, error))?;
        io::checked(
            "set nonblocking",
            inner.as_fd(),
            inner.set_nonblocking(true),
        )?;
        Ok(Self { inner })
    }
    /// Returns the local socket address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        io::checked("local address", self.inner.as_fd(), self.inner.local_addr())
    }
    /// Sets the default numeric peer without establishing a reliable connection.
    pub fn connect(&self, address: SocketAddr) -> Result<()> {
        io::checked(
            "UDP connect",
            self.inner.as_fd(),
            self.inner.connect(address),
        )
    }
    /// Receives one datagram and its source, parking when no data is available.
    pub fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || self.inner.recv_from(buffer),
        )
    }
    /// Sends one datagram to the supplied numeric address.
    pub fn send_to(&self, buffer: &[u8], address: SocketAddr) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || self.inner.send_to(buffer, address),
        )
    }
    /// Receives a datagram from the connected peer.
    pub fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::READABLE,
            SuspensionReason::IoRead,
            || self.inner.recv(buffer),
        )
    }
    /// Sends a datagram to the connected peer.
    pub fn send(&self, buffer: &[u8]) -> Result<usize> {
        io::operation(
            self.inner.as_fd(),
            zio::Interest::WRITABLE,
            SuspensionReason::IoWrite,
            || self.inner.send(buffer),
        )
    }
}

#[cfg(test)]
#[path = "udp_test.rs"]
mod udp_test;
