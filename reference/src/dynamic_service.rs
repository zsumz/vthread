//! A virtual acceptor discovers bounded, transferable connection handlers.

use crate::{
    app,
    dynamic_protocol::{self, Handled},
};
use std::{
    net::SocketAddr,
    thread::ThreadId,
    time::{Duration, Instant},
};
use vthread::{
    Error, JoinHandle, Result, Runtime, SpawnOptions, Spawner, channel, net::TcpListener,
    parking::park_pair,
};

const IN_FLIGHT: usize = 4;

#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) accepted: usize,
    pub(crate) echoed: usize,
    pub(crate) blocking: usize,
    pub(crate) cancelled: usize,
    pub(crate) expired: usize,
    pub(crate) used_another_carrier: bool,
}

impl Report {
    fn collect(&mut self, mut task: JoinHandle<Result<Handled>>, acceptor: ThreadId) -> Result<()> {
        let handled = task.join()??;
        self.used_another_carrier |= handled.carrier != acceptor;
        match handled.mode {
            b'e' => self.echoed += 1,
            b'b' => self.blocking += 1,
            b'c' => self.cancelled += 1,
            b'd' => self.expired += 1,
            _ => unreachable!("validated protocol"),
        }
        Ok(())
    }
}

fn serve(listener: TcpListener, spawner: Spawner, connections: usize) -> Result<Report> {
    let acceptor = std::thread::current().id();
    let mut report = Report::default();
    let mut handlers = Vec::with_capacity(IN_FLIGHT);
    for _ in 0..connections {
        // Reap results to release retained completion records and apply backpressure.
        if handlers.len() == IN_FLIGHT {
            report.collect(handlers.remove(0), acceptor)?;
        }
        let (socket, _) = listener.accept()?;
        let mut mode = [0];
        socket.read_exact(&mut mode)?;
        let mode = mode[0];
        let deadline = Instant::now()
            + if mode == b'd' {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(5)
            };
        let (started, ready) = channel::bounded(1)?;
        let task = spawner.spawn_with(
            SpawnOptions::default().deadline(deadline),
            "connection",
            move || dynamic_protocol::handle(socket, mode, started),
        )?;
        if mode == b'c' {
            ready.recv()?;
            task.cancel();
        }
        handlers.push(task);
        report.accepted += 1;
    }
    // Finite test protocol stops accepting first, then drains every admitted handler.
    drop(listener);
    for task in handlers {
        report.collect(task, acceptor)?;
    }
    Ok(report)
}

pub(crate) fn run(rounds: usize) -> std::result::Result<Report, app::Failure> {
    assert!((1..=4).contains(&rounds));
    app::run(
        Runtime::builder()
            .carriers(2)
            .max_vthreads(32)
            .stack_cache_capacity(8),
        |runtime| {
            let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
            let address = listener.local_addr()?;
            let report = runtime.run_scope(|scope| {
                let spawner = scope.spawner();
                let mut acceptor =
                    scope.spawn("acceptor", move || serve(listener, spawner, rounds * 4))?;
                let mut clients = Vec::with_capacity(rounds * 4);
                let mut wakes = Vec::with_capacity(rounds * 4);
                for index in 0..rounds * 4 {
                    let mode = b"ebcd"[index % 4];
                    let (park, wake) = park_pair();
                    wakes.push(wake);
                    clients.push(scope.spawn_with(
                        SpawnOptions::default().deadline(Instant::now() + Duration::from_secs(8)),
                        "client",
                        move || {
                            park.park()?;
                            dynamic_protocol::client(address, mode, index as u64)
                        },
                    )?);
                }
                // All initial packets are reserved before clients connect. The first handler
                // therefore has a less-loaded carrier available than its acceptor's carrier.
                for wake in wakes {
                    wake.unpark();
                }
                for mut client in clients {
                    client.join()??;
                }
                let report = acceptor.join()??;
                assert!(report.used_another_carrier);
                assert_eq!(runtime.snapshot().active(), 0);
                Ok::<_, Error>(report)
            })?;
            Ok(report)
        },
    )
}

#[cfg(test)]
#[path = "dynamic_service_test.rs"]
mod dynamic_service_test;
