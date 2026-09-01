//! Standalone reference application built only against vthread's public API.

#![forbid(unsafe_code)]

mod api_checks;
mod app;
mod discovered_work;
mod dynamic_protocol;
mod dynamic_service;
mod notification_worker;
mod owner_checks;

use std::{
    net::SocketAddr,
    sync::mpsc,
    time::{Duration, Instant},
};
use vthread::{
    Error, Result, Runtime, ScopeOptions, blocking, channel, lifecycle::ShutdownOutcome,
};

fn pipeline(messages: u64) -> Result<u64> {
    let runtime = Runtime::builder()
        .carriers(2)
        .max_vthreads(8)
        .stack_cache_capacity(8)
        .build()?;
    let (sender, receiver) = channel::bounded_with_wait_capacity(4, 8)?;
    let total = runtime.run_scope(|scope| {
        let mut consumer = scope.spawn("sum", move || {
            let mut total = 0;
            loop {
                match receiver.recv() {
                    Ok(value) => total += value,
                    Err(Error::Closed) => return Ok::<_, Error>(total),
                    Err(error) => return Err(error),
                }
            }
        })?;
        scope
            .spawn("produce", move || {
                for value in 0..messages {
                    sender.send(value).map_err(|error| error.into_parts().0)?;
                }
                Ok::<_, Error>(())
            })?
            .join()??;
        consumer.join()?
    })?;
    runtime.shutdown()?;
    Ok(total)
}

fn tcp_echo(payload: Vec<u8>) -> Result<Vec<u8>> {
    let runtime = Runtime::new()?;
    let listener = vthread::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    let address = listener.local_addr()?;
    let length = payload.len();
    // The inherited deadline bounds all virtual connection/read/write waits.
    let options = ScopeOptions::default().deadline(Instant::now() + Duration::from_secs(5));
    let echoed = runtime.run_scope_with(options, |scope| {
        let mut server = scope.spawn("echo", move || {
            let (socket, _) = listener.accept()?;
            let mut bytes = vec![0; length];
            socket.read_exact(&mut bytes)?;
            socket.write_all(&bytes)
        })?;
        let mut client = scope.spawn("client", move || {
            let socket = vthread::net::TcpStream::connect(address)?;
            socket.write_all(&payload)?;
            let mut echoed = vec![0; payload.len()];
            socket.read_exact(&mut echoed)?;
            Ok::<_, Error>(echoed)
        })?;
        let echoed = client.join()??;
        server.join()??;
        Ok(echoed)
    })?;
    runtime.shutdown()?;
    Ok(echoed)
}

fn native_shutdown() -> Result<()> {
    let runtime = Runtime::new()?;
    let (release, gate) = mpsc::sync_channel(1);
    let (started, entered) = mpsc::sync_channel(1);
    runtime.run_scope(|scope| {
        let mut native = scope.spawn("owned native call", move || {
            blocking::run(move || {
                started.send(()).expect("OS owner");
                gate.recv_timeout(Duration::from_secs(5))
                    .expect("release native work");
            })
        })?;
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("native worker started");
        let outcome = runtime.shutdown_until(Instant::now())?;
        // Retain the runtime, inspect remaining work, and release the native operation.
        release.send(()).expect("native worker waiting");
        let ShutdownOutcome::TimedOut(snapshot) = outcome else {
            panic!("live job lost");
        };
        assert_eq!(snapshot.services().blocking_running(), 1);
        assert!(!snapshot.accepting());
        runtime.shutdown()?;
        assert!(matches!(native.join(), Err(Error::TaskAborted { .. })));
        Ok(())
    })
}

fn observations() -> Result<()> {
    use vthread::error::CapacityResource;
    let runtime = Runtime::builder()
        .max_vthreads(1)
        .stack_cache_capacity(1)
        .build()?;
    runtime.run_scope(|scope| {
        let mut task = scope.spawn("owned result", || 42)?;
        task.wait()?;
        assert!(matches!(
            scope.spawn("over capacity", || ()),
            Err(Error::Capacity {
                resource: CapacityResource::Tasks,
                limit: 1
            })
        ));
        let snapshot = scope.runtime_snapshot();
        assert_eq!(snapshot.runtime_id(), runtime.id());
        assert_eq!(snapshot.tasks()[0].name(), "owned result");
        assert_eq!(task.take_result()?, 42);
        assert!(matches!(task.take_result(), Err(Error::ResultAlreadyTaken)));
        Ok(())
    })?;
    let (sender, receiver) = channel::bounded_with_wait_capacity(1, 1)?;
    drop(receiver);
    let rejected = sender.try_send(42).unwrap_err();
    assert!(matches!(rejected.error(), Error::Closed));
    assert_eq!(rejected.into_inner(), 42);
    runtime.shutdown()?;
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("dynamic-service") {
        println!("{:?}", dynamic_service::run(2)?);
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("worker") {
        println!("{}", notification_worker::run()?);
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("walk") {
        println!("{}", discovered_work::run()?);
        return Ok(());
    }
    assert_eq!(pipeline(1_000)?, 499_500);
    let payload = b"bounded virtual I/O".to_vec();
    assert_eq!(tcp_echo(payload.clone())?, payload);
    native_shutdown()?;
    observations()?;
    api_checks::verify()?;
    owner_checks::verify()?;
    println!("dynamic service: {:?}", dynamic_service::run(2)?);
    println!("notification worker: {}", notification_worker::run()?);
    println!("discovered work: {}", discovered_work::run()?);
    println!(
        "reference checks passed: pipeline, TCP echo, owned shutdown, observation ownership, error ownership, default waiter budgets"
    );
    Ok(())
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
