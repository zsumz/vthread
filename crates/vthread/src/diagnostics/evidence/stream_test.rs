use super::EvidenceRecvError;
use crate::diagnostics::evidence::{RuntimeEventKind, bounded};
use std::time::Duration;

#[test]
fn blocking_batch_distinguishes_timeout_and_disconnect() {
    let (recorder, mut stream) = bounded(1);
    core::assert_eq!(
        stream.recv_batch_timeout(Duration::ZERO),
        Err(EvidenceRecvError::Timeout)
    );
    drop(recorder);
    core::assert_eq!(
        stream.recv_batch_timeout(Duration::ZERO),
        Err(EvidenceRecvError::Disconnected)
    );
}

#[test]
fn blocking_batch_is_capacity_bounded_and_sequence_ordered() {
    let (recorder, mut stream) = bounded(3);
    let runtime = crate::identity::RuntimeId::next();
    for phase in [
        crate::ShutdownPhase::Requested,
        crate::ShutdownPhase::JoiningCarriers,
        crate::ShutdownPhase::JoiningReadiness,
    ] {
        recorder.record(runtime, RuntimeEventKind::ShutdownAdvanced { phase });
    }

    let events = stream.recv_batch_timeout(Duration::from_secs(1)).unwrap();
    core::assert_eq!(events.len(), 3);
    core::assert_eq!(events[0].sequence().get(), 0);
    core::assert_eq!(events[2].sequence().get(), 2);
    core::assert_eq!(stream.status().pending(), 0);
}
