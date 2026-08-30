//! Explicit nonblocking socket I/O backed by zio readiness waits.
//!
//! Read/write/accept operations require a virtual caller. Numeric-address binding,
//! socket options, and address inspection may also be used by OS callers. Hostname
//! resolution is explicit and delegated; socket methods never resolve names.
//! Readiness is advisory: every wake retries the actual nonblocking operation.

//!
//! ```
//! use vthread::{Runtime, net::{TcpListener, TcpStream}};
//! let listener = TcpListener::bind(([127, 0, 0, 1], 0).into())?;
//! let address = listener.local_addr()?;
//! let runtime = Runtime::new()?;
//! runtime.scope(|scope| {
//!     let server = scope.spawn("echo", move || {
//!         let (stream, _) = listener.accept()?;
//!         let mut data = [0; 4];
//!         stream.read_exact(&mut data)?;
//!         stream.write_all(&data)
//!     })?;
//!     let client = scope.spawn("client", move || {
//!         let stream = TcpStream::connect(address)?;
//!         stream.write_all(b"ping")?;
//!         let mut data = [0; 4];
//!         stream.read_exact(&mut data)?;
//!         assert_eq!(data, *b"ping");
//!         Ok::<_, vthread::Error>(())
//!     })?;
//!     client.join()??;
//!     server.join()??;
//!     Ok(())
//! })?;
//! # Ok::<(), vthread::Error>(())
//! ```

mod connect;
mod io;
mod tcp;
mod udp;
pub mod unix;

pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;

use crate::{Error, Result, SuspensionReason, blocking};
use std::net::{SocketAddr, ToSocketAddrs};

/// Resolves a hostname on the bounded native pool, rejecting more than `limit`
/// results instead of silently truncating them. Resolver allocation is OS-owned.
/// Cancellation cannot interrupt a resolver call already in progress.
pub fn lookup_host(host: impl Into<String>, port: u16, limit: usize) -> Result<Vec<SocketAddr>> {
    if limit == 0 || limit == usize::MAX {
        return Err(Error::invalid_configuration(
            "address_limit",
            "must be positive and below usize::MAX",
        ));
    }
    let host = host.into();
    blocking::run_for(SuspensionReason::Dns, move || {
        let addresses = (host.as_str(), port)
            .to_socket_addrs()?
            .take(limit + 1)
            .collect::<Vec<_>>();
        if addresses.len() > limit {
            return Err(Error::LimitExceeded {
                resource: "resolved addresses",
                limit,
            });
        }
        Ok(addresses)
    })?
}

#[cfg(test)]
#[path = "net_test.rs"]
mod net_test;
