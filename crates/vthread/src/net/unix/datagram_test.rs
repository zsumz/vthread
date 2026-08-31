use crate::{Runtime, net::unix::UnixDatagram};

#[test]
fn unix_datagrams_wait_without_losing_packet_boundaries() {
    let runtime = Runtime::new().unwrap();
    let (receiver, sender) = UnixDatagram::pair().unwrap();
    runtime
        .run_scope(|scope| {
            let mut read = scope.spawn("read", move || {
                let mut buffer = [0; 4];
                assert_eq!(receiver.recv(&mut buffer)?, 4);
                assert_eq!(buffer, *b"data");
                Ok::<_, crate::Error>(())
            })?;
            scope
                .spawn("write", move || sender.send(b"data"))?
                .join()??;
            read.join()??;
            Ok(())
        })
        .unwrap();
}
