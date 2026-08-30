use crate::{Runtime, net::UdpSocket};

#[test]
fn udp_preserves_datagram_boundaries_and_numeric_peer_addresses() {
    let runtime = Runtime::new().unwrap();
    let server = UdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = server.local_addr().unwrap();
    runtime
        .scope(|scope| {
            let receiver = scope.spawn("recv", move || {
                let mut buffer = [0; 8];
                let (count, peer) = server.recv_from(&mut buffer)?;
                assert_eq!(&buffer[..count], b"packet");
                server.send_to(&buffer[..count], peer)?;
                Ok::<_, crate::Error>(())
            })?;
            scope
                .spawn("send", move || {
                    let socket = UdpSocket::bind("127.0.0.1:0".parse().unwrap())?;
                    socket.connect(address)?;
                    assert_eq!(socket.send(b"packet")?, 6);
                    let mut buffer = [0; 8];
                    assert_eq!(socket.recv(&mut buffer)?, 6);
                    assert_eq!(&buffer[..6], b"packet");
                    Ok::<_, crate::Error>(())
                })?
                .join()??;
            receiver.join()??;
            Ok(())
        })
        .unwrap();
}
