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

    pub(crate) fn observe_routed_pair(&self) -> (Observation, Observation) {
        // Routing is visible before the publisher sends its diagnostic event.
        // The owner can therefore report deferral first. Accept exactly these
        // two events, without treating delivery order as a protocol edge.
        let first = self.next_observation();
        let second = self.next_observation();
        let (published, deferred) = match (first.stage, second.stage) {
            (Stage::NoticePublished, Stage::OwnerDeferred) => (first, second),
            (Stage::OwnerDeferred, Stage::NoticePublished) => (second, first),
            pair => panic!("unexpected route publication pair: {pair:?}"),
        };
        assert_eq!(
            published.token, deferred.token,
            "same exact wait generation"
        );
        (published, deferred)
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

fn observed_pair(stages: [Stage; 2], tokens: [ParkToken; 2]) -> PausedPublication {
    let (events, received) = mpsc::channel();
    for (stage, token) in stages.into_iter().zip(tokens) {
        events
            .send(Observation {
                stage,
                token,
                thread: std::thread::current().id(),
                elapsed: Duration::ZERO,
            })
            .unwrap();
    }
    PausedPublication {
        events: received,
        resume: None,
    }
}

#[test]
fn a_routed_pair_preserves_evidence_when_owner_observation_arrives_first() {
    let token = ParkToken::new(7, 11);
    for stages in [
        [Stage::NoticePublished, Stage::OwnerDeferred],
        [Stage::OwnerDeferred, Stage::NoticePublished],
    ] {
        let pause = observed_pair(stages, [token; 2]);
        let (published, deferred) = pause.observe_routed_pair();
        assert_eq!(published.stage, Stage::NoticePublished);
        assert_eq!(deferred.stage, Stage::OwnerDeferred);
        assert_eq!(published.token, token);
        assert_eq!(deferred.token, token);
    }
}

#[test]
#[should_panic(expected = "same exact wait generation")]
fn a_routed_pair_cannot_combine_different_generations() {
    observed_pair(
        [Stage::NoticePublished, Stage::OwnerDeferred],
        [ParkToken::new(7, 11), ParkToken::new(7, 12)],
    )
    .observe_routed_pair();
}

#[test]
#[should_panic(expected = "unexpected route publication pair")]
fn a_routed_pair_cannot_substitute_mounted_spinning_for_owner_deferral() {
    observed_pair(
        [Stage::NoticePublished, Stage::FinishWaiting],
        [ParkToken::new(7, 11); 2],
    )
    .observe_routed_pair();
}
