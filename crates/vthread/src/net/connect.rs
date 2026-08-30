//! Nonblocking TCP connection establishment and SO_ERROR verification.

use crate::{Result, SuspensionReason, sync::wait::Wait};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{io, net::SocketAddr, os::fd::AsFd};

pub(super) fn tcp(address: SocketAddr) -> Result<std::net::TcpStream> {
    let _reason = Wait::enter(SuspensionReason::IoConnect)?;
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    socket.set_nonblocking(true)?;
    match socket.connect(&SockAddr::from(address)) {
        Ok(()) => return Ok(socket.into()),
        Err(error) if pending(&error) => {}
        Err(error) => return Err(error.into()),
    }
    loop {
        super::io::wait(socket.as_fd(), zio::Interest::WRITABLE)?;
        if let Some(error) = socket.take_error()? {
            return Err(error.into());
        }
        match socket.peer_addr() {
            Ok(_) => return Ok(socket.into()),
            Err(error) if error.kind() == io::ErrorKind::NotConnected || pending(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn pending(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::EINPROGRESS | libc::EALREADY | libc::EINTR)
    ) || error.kind() == io::ErrorKind::WouldBlock
}

#[cfg(test)]
#[path = "connect_test.rs"]
mod connect_test;
