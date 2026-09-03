use crate::{
    Error, Runtime,
    net::{TcpListener, TcpStream},
};
use std::net::Shutdown;

#[test]
fn try_io_reports_would_block_without_registering_readiness() {
    use std::{
        io::{Read, Write},
        sync::mpsc,
        thread,
    };

    let runtime = Runtime::new().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let (probed, continue_peer) = mpsc::sync_channel(0);
    let peer = thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        continue_peer.recv().unwrap();
        let mut byte = [0; 1];
        stream.read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"p");
        stream.write_all(b"x").unwrap();
    });
    runtime
        .run_scope(|scope| {
            scope
                .spawn("probe", move || {
                    let (stream, _) = listener.accept()?;
                    assert!(matches!(
                        stream.try_read(&mut [0; 1]),
                        Err(Error::WouldBlock)
                    ));
                    assert_eq!(stream.try_write(b"p")?, 1);
                    assert_eq!(
                        context_readiness_waits(),
                        0,
                        "a nonblocking probe must not subscribe to readiness"
                    );
                    probed.send(()).unwrap();
                    stream.read_exact(&mut [0; 1])
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
    peer.join().unwrap();
}

fn context_readiness_waits() -> usize {
    let mounted = crate::context::current().unwrap();
    mounted
        .execution()
        .unwrap()
        .shared()
        .services
        .get()
        .unwrap()
        .snapshot()
        .readiness_waits()
}

#[test]
fn tcp_accept_connect_echo_and_eof_work_on_one_and_four_carriers() {
    for carriers in [1, 4] {
        let runtime = Runtime::builder().carriers(carriers).build().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        runtime
            .run_scope(|scope| {
                let mut server = scope.spawn("accept", move || {
                    let (stream, _) = listener.accept()?;
                    let mut input = [0; 4];
                    stream.read_exact(&mut input)?;
                    stream.write_all(&input)?;
                    stream.shutdown(Shutdown::Write)?;
                    Ok::<_, crate::Error>(input)
                })?;
                let mut client = scope.spawn("connect", move || {
                    let stream = TcpStream::connect(address)?;
                    stream.set_nodelay(true)?;
                    stream.write_all(b"ping")?;
                    let mut output = [0; 4];
                    stream.read_exact(&mut output)?;
                    assert_eq!(stream.read(&mut [0; 1])?, 0);
                    Ok::<_, crate::Error>(output)
                })?;
                assert_eq!(server.join()??, *b"ping");
                assert_eq!(client.join()??, *b"ping");
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn slow_external_io_does_not_trigger_the_ownerless_park_stall_policy() {
    use std::{io::Write, thread, time::Duration};
    let runtime = Runtime::builder()
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(10)))
        .build()
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let remote = thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        thread::park_timeout(Duration::from_millis(50));
        stream.write_all(b"x").unwrap();
    });
    runtime
        .run_scope(|scope| {
            scope
                .spawn("slow-peer", move || {
                    let (stream, _) = listener.accept()?;
                    stream.read_exact(&mut [0; 1])
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
    remote.join().unwrap();
}

#[test]
fn shared_stream_keeps_concurrent_readiness_registrations_distinct() {
    use crate::support_test::until;
    use std::{io::Write, sync::Arc, sync::mpsc, thread};

    let runtime = Runtime::new().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = mpsc::sync_channel(0);
    let peer = thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        released.recv().unwrap();
        stream.write_all(b"ab").unwrap();
    });
    runtime
        .run_scope(|scope| {
            let stream = scope
                .spawn("accept", move || listener.accept().map(|pair| pair.0))?
                .join()??;
            let stream = Arc::new(stream);
            let mut readers = Vec::new();
            for _ in 0..2 {
                let stream = Arc::clone(&stream);
                readers.push(scope.spawn("reader", move || {
                    let mut byte = [0; 1];
                    stream.read_exact(&mut byte)?;
                    Ok::<_, crate::Error>(byte[0])
                })?);
            }
            until(|| runtime.snapshot().services.readiness_waits == 2);
            release.send(()).unwrap();
            let mut bytes = Vec::new();
            for mut reader in readers {
                bytes.push(reader.join()??);
            }
            bytes.sort_unstable();
            assert_eq!(bytes, b"ab");
            Ok(())
        })
        .unwrap();
    peer.join().unwrap();
}
