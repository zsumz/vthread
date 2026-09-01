//! A bounded binary protocol for the dynamic-service consumer.

use std::{net::SocketAddr, thread::ThreadId, time::Duration};
use vthread::{Error, Result, blocking, channel::Sender, net::TcpStream};

#[derive(Debug)]
pub(crate) struct Handled {
    pub(crate) mode: u8,
    pub(crate) carrier: ThreadId,
}

pub(crate) fn handle(socket: TcpStream, mode: u8, started: Sender<()>) -> Result<Handled> {
    let carrier = std::thread::current().id();
    if mode == b'c' {
        started.send(()).map_err(|error| error.into_parts().0)?;
    }
    match mode {
        b'e' | b'b' => {
            let mut bytes = [0; 8];
            socket.read_exact(&mut bytes)?;
            if mode == b'b' {
                bytes = blocking::run(move || {
                    assert_ne!(std::thread::current().id(), carrier);
                    (u64::from_be_bytes(bytes) + 1).to_be_bytes()
                })?;
            }
            socket.write_all(&bytes)?;
        }
        b'c' => assert!(matches!(
            vthread::sleep(Duration::from_secs(10)),
            Err(Error::Cancelled)
        )),
        b'd' => assert!(matches!(
            vthread::sleep(Duration::from_secs(10)),
            Err(Error::DeadlineExceeded)
        )),
        _ => unreachable!("consumer supplied an invalid mode"),
    }
    assert_eq!(std::thread::current().id(), carrier);
    Ok(Handled { mode, carrier })
}

pub(crate) fn client(address: SocketAddr, mode: u8, value: u64) -> Result<()> {
    let socket = TcpStream::connect(address)?;
    socket.write_all(&[mode])?;
    if matches!(mode, b'e' | b'b') {
        socket.write_all(&value.to_be_bytes())?;
        let mut response = [0; 8];
        socket.read_exact(&mut response)?;
        assert_eq!(
            u64::from_be_bytes(response),
            value + u64::from(mode == b'b')
        );
    } else {
        // The cancelled/expired handler closes its owned connection on reclamation.
        assert_eq!(socket.read(&mut [0])?, 0);
    }
    Ok(())
}

#[cfg(test)]
#[path = "dynamic_protocol_test.rs"]
mod dynamic_protocol_test;
