use crate::{
    Runtime,
    net::unix::{UnixListener, UnixStream},
    support_test::until,
};
use std::{net::Shutdown, sync::Arc};

#[test]
fn stream_backpressure_parks_the_writer_without_stopping_the_reader() {
    let runtime = Runtime::new().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    socket2::SockRef::from(&writer.inner)
        .set_send_buffer_size(4096)
        .unwrap();
    runtime
        .scope(|scope| {
            let send = scope.spawn("write", move || writer.write_all(&vec![7; 1024 * 1024]))?;
            until(|| runtime.snapshot().services.readiness_waits == 1);
            let receive = scope.spawn("read", move || {
                let mut buffer = vec![0; 1024 * 1024];
                reader.read_exact(&mut buffer)?;
                assert!(buffer.iter().all(|byte| *byte == 7));
                Ok::<_, crate::Error>(())
            })?;
            send.join()??;
            receive.join()??;
            Ok(())
        })
        .unwrap();
}

#[test]
fn unix_path_connect_is_delegated_and_accept_is_readiness_driven() {
    let path = std::env::temp_dir().join(format!("vthread-unix-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            let server = scope.spawn("accept", move || listener.accept().map(|pair| pair.0))?;
            let path = path.clone();
            let client = scope.spawn("connect", move || UnixStream::connect(path))?;
            let server = server.join()??;
            let client = client.join()??;
            client.shutdown(Shutdown::Write)?;
            scope
                .spawn("eof", move || server.read(&mut [0; 1]))?
                .join()??;
            Ok(())
        })
        .unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn closing_a_shared_read_direction_wakes_its_parked_reader() {
    let runtime = Runtime::new().unwrap();
    let (reader, _writer) = UnixStream::pair().unwrap();
    let reader = Arc::new(reader);
    runtime
        .scope(|scope| {
            let socket = Arc::clone(&reader);
            let task = scope.spawn("read", move || socket.read(&mut [0; 1]))?;
            until(|| runtime.snapshot().services.readiness_waits == 1);
            reader.shutdown(Shutdown::Read)?;
            assert_eq!(task.join()??, 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn competing_readers_recheck_readiness_and_each_consume_one_byte() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    let reader = Arc::new(reader);
    runtime
        .scope(|scope| {
            let mut reads = Vec::new();
            for _ in 0..2 {
                let reader = Arc::clone(&reader);
                reads.push(scope.spawn("reader", move || {
                    let mut byte = [0; 1];
                    reader.read_exact(&mut byte)?;
                    Ok::<_, crate::Error>(byte[0])
                })?);
            }
            until(|| runtime.snapshot().services.readiness_waits == 2);
            scope
                .spawn("writer", move || writer.write_all(b"ab"))?
                .join()??;
            let mut values = Vec::new();
            for read in reads {
                values.push(read.join()??);
            }
            values.sort_unstable();
            assert_eq!(values, b"ab");
            Ok(())
        })
        .unwrap();
}
