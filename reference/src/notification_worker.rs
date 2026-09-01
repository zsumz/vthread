//! Explicit supervisors own workers; the runtime owns unfinished native provider calls.

use crate::app;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use vthread::{
    Error, Result, Runtime, blocking, channel,
    lifecycle::{ShutdownOutcome, SupervisorShutdownOutcome},
};

struct ProviderGate {
    started: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    finished: Arc<AtomicBool>,
}

struct Job {
    id: u64,
    gate: Option<ProviderGate>,
}

#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) delivered: usize,
    pub(crate) retained_native_jobs: usize,
    pub(crate) supervisor_timed_out: bool,
    pub(crate) native_finished: bool,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} delivered; {} retained native jobs; supervisor timed out: {}; native completion: {}",
            self.delivered,
            self.retained_native_jobs,
            self.supervisor_timed_out,
            self.native_finished
        )
    }
}

fn worker(jobs: channel::Receiver<Job>, delivered: channel::Sender<u64>) -> Result<()> {
    loop {
        let job = match jobs.recv() {
            Ok(job) => job,
            Err(Error::Closed) => return Ok(()),
            Err(error) => return Err(error),
        };
        let id = blocking::run(move || {
            if let Some(gate) = job.gate {
                gate.started.send(()).unwrap();
                gate.release.recv_timeout(Duration::from_secs(5)).unwrap();
                gate.finished.store(true, Ordering::SeqCst);
            }
            job.id
        })?;
        delivered.send(id).map_err(|error| error.into_parts().0)?;
    }
}

pub(crate) fn run() -> std::result::Result<Report, app::Failure> {
    let finished = Arc::new(AtomicBool::new(false));
    let native_finished = Arc::clone(&finished);
    let mut report = app::run(
        Runtime::builder()
            .carriers(2)
            .max_vthreads(16)
            .stack_cache_capacity(8),
        |runtime| {
            let mut supervisor = runtime.supervisor()?;
            let (jobs, incoming) = channel::bounded(4)?;
            let (delivered, results) = channel::bounded(4)?;
            let mut workers = Vec::with_capacity(2);
            for _ in 0..2 {
                let incoming = incoming.clone();
                let delivered = delivered.clone();
                workers.push(
                    supervisor.spawn("notification worker", move || worker(incoming, delivered))?,
                );
            }
            drop(incoming);
            drop(delivered);
            let producer = jobs.clone();
            let count = runtime.run_scope(|scope| {
                scope
                    .spawn("producer and observer", move || {
                        for id in 0..4 {
                            producer
                                .send(Job { id, gate: None })
                                .map_err(|error| error.into_parts().0)?;
                        }
                        let mut ids = Vec::with_capacity(4);
                        for _ in 0..4 {
                            ids.push(results.recv()?);
                        }
                        ids.sort_unstable();
                        assert_eq!(ids, [0, 1, 2, 3]);
                        Ok::<_, Error>(ids.len())
                    })?
                    .join()?
            })?;

            let (started, entered) = mpsc::sync_channel(1);
            let (release, gate) = mpsc::sync_channel(1);
            jobs.try_send(Job {
                id: 99,
                gate: Some(ProviderGate {
                    started,
                    release: gate,
                    finished: native_finished,
                }),
            })
            .map_err(|error| error.into_parts().0)?;
            entered.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(jobs);
            let admission = supervisor.spawner();
            supervisor.request_shutdown();
            assert!(matches!(
                admission.spawn("too late", || ()),
                Err(Error::ScopeClosed)
            ));
            let supervisor_timed_out = match supervisor.shutdown_until(Instant::now())? {
                SupervisorShutdownOutcome::TimedOut(timeout) => {
                    assert_eq!(timeout.supervisor_id(), supervisor.id());
                    assert!(timeout.tasks().all(|task| task.scope() == supervisor.id()));
                    true
                }
                SupervisorShutdownOutcome::Complete(_) => false,
                _ => unreachable!("new supervisor outcome requires an explicit consumer decision"),
            };
            // Supervised stacks can be reclaimed while an admitted provider still runs.
            let timeout = runtime.shutdown_until(Instant::now())?;
            let retained_native_jobs = match &timeout {
                ShutdownOutcome::TimedOut(snapshot) => snapshot.services().blocking_running(),
                _ => 0,
            };
            // Always release native ownership before making assertions or unwinding.
            release.send(()).unwrap();
            assert_eq!(
                retained_native_jobs, 1,
                "native provider was not retained: {timeout:?}"
            );
            assert!(matches!(
                supervisor.shutdown_until(Instant::now() + Duration::from_secs(5))?,
                SupervisorShutdownOutcome::Complete(_)
            ));
            for mut worker in workers {
                // Stop may reclaim a waiting stack or let its cooperative error return.
                match worker.join() {
                    Ok(Ok(()))
                    | Ok(Err(Error::Cancelled | Error::RuntimeStopped | Error::Closed))
                    | Err(Error::TaskAborted { .. }) => {}
                    result => panic!("unexpected worker shutdown result: {result:?}"),
                }
            }
            Ok(Report {
                delivered: count,
                retained_native_jobs,
                supervisor_timed_out,
                native_finished: false,
            })
        },
    )?;
    report.native_finished = finished.load(Ordering::SeqCst);
    assert!(report.native_finished);
    Ok(report)
}

#[cfg(test)]
#[path = "notification_worker_test.rs"]
mod notification_worker_test;
