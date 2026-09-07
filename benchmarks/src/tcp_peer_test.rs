use super::EchoServer;
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

#[test]
fn echo_peer_preserves_every_requested_byte() {
    let server = EchoServer::start(1, 3).unwrap();
    let mut client = TcpStream::connect_timeout(&server.address(), Duration::from_secs(2)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    for value in [0, 127, 255] {
        client.write_all(&[value]).unwrap();
        let mut returned = [0];
        client.read_exact(&mut returned).unwrap();
        assert_eq!(returned, [value]);
    }
    drop(client);
    server.finish().unwrap();
}

#[test]
fn early_disconnect_is_reported_as_an_incomplete_round() {
    let server = EchoServer::start(1, 1).unwrap();
    drop(TcpStream::connect_timeout(&server.address(), Duration::from_secs(2)).unwrap());
    assert!(
        server
            .finish()
            .unwrap_err()
            .contains("read TCP benchmark byte")
    );
}
