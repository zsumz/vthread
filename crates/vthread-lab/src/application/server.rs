//! Fixed virtual workers and a bounded TCP admission queue, owned by one supervisor.

use super::{
    failure, protocol,
    state::{self, Connection, Shared},
};
use std::{
    io::Write,
    net::SocketAddr,
    time::{Duration, Instant},
};
use vthread::{
    Error, JoinHandle, Result, Runtime, channel, diagnostics::SuspensionReason,
    diagnostics::TaskStatus, lifecycle::Supervisor, net::TcpListener,
};

pub(crate) struct Service {
    pub address: SocketAddr,
    counts: Shared,
    tasks: Vec<JoinHandle<Result<()>>>,
}

pub(crate) fn start(
    owner: &Supervisor<'_>,
    workers: usize,
    capacity: usize,
    timeout: Duration,
) -> Result<Service> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    let address = listener.local_addr()?;
    let counts = Shared::default();
    let (sender, receiver) =
        channel::bounded_with_wait_capacity::<Connection>(capacity, workers + 2)?;
    let mut tasks = Vec::with_capacity(workers + 1);
    for _ in 0..workers {
        let receiver = receiver.clone();
        tasks.push(owner.spawn("application-worker", move || {
            loop {
                let mut connection = match receiver.recv() {
                    Ok(connection) => connection,
                    Err(Error::Cancelled | Error::Closed) => return Ok(()),
                    Err(error) => return Err(error),
                };
                connection.activate();
                serve(&connection, timeout)?;
            }
        })?);
    }
    drop(receiver);
    let shared = counts.clone();
    tasks.push(owner.spawn("application-accept", move || {
        loop {
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(Error::Cancelled | Error::RuntimeStopped) => return Ok(()),
                Err(error) => return Err(error),
            };
            stream.set_nodelay(true)?;
            let connection = Connection::new(stream, shared.clone());
            if let Err(rejected) = sender.try_send(connection) {
                let (error, connection) = rejected.into_parts();
                match error {
                    Error::WouldBlock => {
                        state::change(&shared, |state| state.rejected += 1);
                        // One byte fits the fresh socket's send buffer; this is still explicit I/O.
                        let _ = connection.stream.write_all(&[protocol::BUSY]);
                    }
                    Error::Closed => return Ok(()),
                    error => return Err(error),
                }
            }
        }
    })?);
    Ok(Service {
        address,
        counts,
        tasks,
    })
}

fn serve(connection: &Connection, timeout: Duration) -> Result<()> {
    let result: Result<()> = (|| {
        connection.stream.write_all(&[protocol::READY])?;
        loop {
            let received = vthread::local_scope_with_deadline(Instant::now() + timeout, |scope| {
                scope
                    .spawn("application-request", || {
                        protocol::exchange(&connection.stream)
                    })?
                    .join()?
            })?;
            if !received {
                return Ok(());
            }
            state::change(&connection.state, |state| state.requests += 1);
        }
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let Some(expected) = failure::expected_service(&error) else {
                return Err(error);
            };
            state::change(&connection.state, |state| match expected {
                failure::ServiceFailure::Deadline => state.deadlines += 1,
                failure::ServiceFailure::Malformed => state.malformed += 1,
                failure::ServiceFailure::Disconnected => state.disconnected += 1,
            });
            Ok(())
        }
    }
}

impl Service {
    pub(crate) fn report(&self, runtime: &Runtime, event: &str) -> Result<()> {
        let state = state::change(&self.counts, |state| *state);
        let snapshot = runtime.snapshot();
        let waits = |reason| {
            snapshot
                .tasks()
                .iter()
                .filter(|task| task.status() == TaskStatus::Suspended(reason))
                .count()
        };
        let io = snapshot.services();
        writeln!(
            std::io::stdout(),
            concat!(
                "{{\"event\":\"{}\",\"accepted\":{},\"rejected\":{},\"closed\":{},\"requests\":{},",
                "\"deadlines\":{},\"malformed\":{},\"disconnected\":{},\"pending\":{},\"active\":{},",
                "\"peak_pending\":{},\"peak_active\":{},\"io_readers\":{},\"io_writers\":{},",
                "\"runtime_active\":{},\"runtime_parked\":{},\"timers\":{},\"readiness\":{},",
                "\"registered\":{},\"native\":{},\"panicked\":{},\"spawned\":{},\"completed\":{},",
                "\"aborted\":{},\"stack_allocated\":{},\"stack_cached\":{},\"shutdown\":\"{:?}\"}}"
            ),
            event,
            state.accepted,
            state.rejected,
            state.closed,
            state.requests,
            state.deadlines,
            state.malformed,
            state.disconnected,
            state.pending,
            state.active,
            state.peak_pending,
            state.peak_active,
            waits(SuspensionReason::IoRead),
            waits(SuspensionReason::IoWrite),
            snapshot.active(),
            snapshot.parked(),
            snapshot.timers(),
            io.readiness_waits(),
            io.readiness_registered(),
            io.blocking_running()
                + io.blocking_queued()
                + io.blocking_completed()
                + io.blocking_discarding(),
            snapshot.stats().panicked(),
            snapshot.stats().admitted(),
            snapshot.stats().completed(),
            snapshot.stats().aborted(),
            snapshot.stacks().allocated(),
            snapshot.stacks().cached(),
            snapshot.shutdown_phase()
        )?;
        std::io::stdout().flush()?;
        Ok(())
    }

    pub(crate) fn join(self) -> Result<()> {
        for mut task in self.tasks {
            // These handles are observed after runtime shutdown; interrupted I/O is expected.
            match task.join().and_then(|result| result) {
                Ok(()) => {}
                Err(error) if failure::expected_shutdown(&error) => {}
                Err(error) => return Err(error),
            }
        }
        let state = state::change(&self.counts, |state| *state);
        assert_eq!(state.accepted, state.closed);
        assert_eq!((state.pending, state.active), (0, 0));
        Ok(())
    }
}

#[cfg(test)]
#[path = "server_test.rs"]
mod server_test;
