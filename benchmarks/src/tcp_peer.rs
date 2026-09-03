use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    thread::{self, JoinHandle},
};

pub(crate) struct EchoServer {
    address: SocketAddr,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl EchoServer {
    pub(crate) fn start(clients: usize, round_trips: usize) -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("bind TCP echo peer: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("read TCP echo address: {error}"))?;
        let worker = thread::Builder::new()
            .name("benchmark-tcp-peer".into())
            .spawn(move || serve(listener, clients, round_trips))
            .map_err(|error| format!("spawn TCP echo peer: {error}"))?;
        Ok(Self {
            address,
            worker: Some(worker),
        })
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn finish(mut self) -> Result<(), String> {
        self.worker
            .take()
            .expect("live TCP echo peer")
            .join()
            .map_err(|_| "TCP echo peer panicked".to_owned())?
    }
}

fn serve(listener: TcpListener, clients: usize, round_trips: usize) -> Result<(), String> {
    let mut connections = Vec::with_capacity(clients);
    for _ in 0..clients {
        let (stream, _) = listener
            .accept()
            .map_err(|error| format!("accept TCP benchmark client: {error}"))?;
        stream
            .set_nodelay(true)
            .map_err(|error| format!("configure TCP echo peer: {error}"))?;
        connections.push(
            thread::Builder::new()
                .name("benchmark-tcp-connection".into())
                .spawn(move || echo(stream, round_trips))
                .map_err(|error| format!("spawn TCP echo connection: {error}"))?,
        );
    }
    for connection in connections {
        connection
            .join()
            .map_err(|_| "TCP echo connection panicked".to_owned())??;
    }
    Ok(())
}

fn echo(mut stream: TcpStream, round_trips: usize) -> Result<(), String> {
    let mut byte = [0_u8; 1];
    for _ in 0..round_trips {
        stream
            .read_exact(&mut byte)
            .map_err(|error| format!("read TCP benchmark byte: {error}"))?;
        stream
            .write_all(&byte)
            .map_err(|error| format!("write TCP benchmark byte: {error}"))?;
    }
    Ok(())
}
