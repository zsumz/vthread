use crate::{
    Runtime,
    net::{TcpListener, TcpStream},
};
use std::net::Shutdown;

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
