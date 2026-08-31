//! Persistent loopback traffic avoids measuring ephemeral-port exhaustion as runtime health.

use std::net::SocketAddr;
use vthread::{
    JoinHandle, Result, Scope,
    net::{TcpListener, TcpStream},
};

pub(crate) struct Pair {
    server: TcpStream,
    client: TcpStream,
}

enum Endpoint {
    Connected(TcpStream),
    Accept(TcpListener),
    Connect(SocketAddr),
}

impl Endpoint {
    fn open(self) -> Result<TcpStream> {
        let stream = match self {
            Self::Connected(stream) => return Ok(stream),
            Self::Accept(listener) => listener.accept()?.0,
            Self::Connect(address) => TcpStream::connect(address)?,
        };
        stream.set_nodelay(true)?;
        Ok(stream)
    }
}

pub(crate) struct Exchange {
    server: JoinHandle<Result<TcpStream>>,
    client: JoinHandle<Result<TcpStream>>,
}

pub(crate) fn start(scope: &Scope<'_>, iteration: u64, previous: Option<Pair>) -> Result<Exchange> {
    let (server, client) = match previous {
        Some(pair) => (
            Endpoint::Connected(pair.server),
            Endpoint::Connected(pair.client),
        ),
        None => {
            let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
            let address = listener.local_addr()?;
            (Endpoint::Accept(listener), Endpoint::Connect(address))
        }
    };
    let server = scope.spawn("soak-tcp-echo", move || {
        let stream = server.open()?;
        let mut bytes = [0; 8];
        stream.read_exact(&mut bytes)?;
        assert_eq!(bytes, iteration.to_be_bytes());
        stream.write_all(&bytes)?;
        Ok(stream)
    })?;
    let client = scope.spawn("soak-tcp-client", move || {
        let stream = client.open()?;
        stream.write_all(&iteration.to_be_bytes())?;
        let mut bytes = [0; 8];
        stream.read_exact(&mut bytes)?;
        assert_eq!(bytes, iteration.to_be_bytes());
        Ok(stream)
    })?;
    Ok(Exchange { server, client })
}

impl Exchange {
    pub(crate) fn finish(mut self) -> Result<Pair> {
        Ok(Pair {
            server: self.server.join()??,
            client: self.client.join()??,
        })
    }
}

#[cfg(test)]
#[path = "network_test.rs"]
mod network_test;
