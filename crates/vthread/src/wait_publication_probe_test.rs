//! Per-wait fault injection. Entirely absent from non-test builds.

use std::{
    io::Write,
    sync::{
        Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

use super::{WaitCell, WaitInner};
use vthread_stack::ParkToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    NoticePublished,
    FinishWaiting,
    FinishSpinning,
    RetireWaiting,
    OwnerDeferred,
    ClaimPublished,
    RetirementDeferred,
}

#[derive(Debug)]
pub(crate) struct Observation {
    pub(crate) stage: Stage,
    pub(crate) token: ParkToken,
    pub(crate) thread: ThreadId,
    pub(crate) elapsed: Duration,
}

pub(crate) struct Probe {
    pause_on: Stage,
    seen: AtomicU8,
    finish_visits: AtomicUsize,
    started: Instant,
    events: Sender<Observation>,
    resume: Mutex<Receiver<()>>,
}

/// Drop releases the publisher, including when a test assertion unwinds. Keep this
/// inside the scoped-thread closure so release precedes its implicit thread joins.
pub(crate) struct PausedPublication {
    events: Receiver<Observation>,
    resume: Option<Sender<()>>,
}

impl PausedPublication {
    pub(crate) fn install(cell: &WaitCell) -> Self {
        Self::install_at(cell, Stage::NoticePublished)
    }

    pub(crate) fn install_at(cell: &WaitCell, pause_on: Stage) -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        assert!(
            cell.state
                .publication_probe
                .set(Probe {
                    pause_on,
                    seen: AtomicU8::new(0),
                    finish_visits: AtomicUsize::new(0),
                    started: Instant::now(),
                    events: events_tx,
                    resume: Mutex::new(resume_rx),
                })
                .is_ok(),
            "one publication probe per wait cell"
        );
        Self {
            events: events_rx,
            resume: Some(resume_tx),
        }
    }

    pub(crate) fn observe(&self, expected: Stage) -> Observation {
        let event = self.next_observation();
        assert_eq!(event.stage, expected);
        event
    }

    pub(crate) fn next_observation(&self) -> Observation {
        let event = self
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("publication stage not reached");
        writeln!(std::io::stdout().lock(), "publication probe: {event:?}").unwrap();
        event
    }

    pub(crate) fn release(&mut self) {
        if let Some(resume) = self.resume.take() {
            let _ = resume.send(());
        }
    }
}

impl WaitCell {
    pub(crate) fn publication_is_held(&self) -> bool {
        self.state.load().is_claimed()
    }
}

impl Drop for PausedPublication {
    fn drop(&mut self) {
        self.release();
    }
}

impl WaitInner {
    pub(super) fn observe_publication(&self, stage: Stage, token: ParkToken) {
        let Some(probe) = self.publication_probe.get() else {
            return;
        };
        let stage = if stage == Stage::FinishWaiting {
            match probe.finish_visits.fetch_add(1, Ordering::Relaxed) {
                0 => Stage::FinishWaiting,
                1_023 => Stage::FinishSpinning,
                _ => return,
            }
        } else {
            stage
        };
        let bit = 1 << stage as u8;
        if probe.seen.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
            return;
        }
        let _ = probe.events.send(Observation {
            stage,
            token,
            thread: std::thread::current().id(),
            elapsed: probe.started.elapsed(),
        });
        if stage == probe.pause_on {
            // Disconnection also releases the pause. Never panic in the claimed
            // interval or let a failed test strand the production state machine.
            let _ = crate::signal::lock(&probe.resume).recv();
        }
    }
}
