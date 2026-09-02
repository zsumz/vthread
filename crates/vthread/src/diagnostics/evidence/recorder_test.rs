use super::bounded;
use crate::diagnostics::evidence::RuntimeEventKind;

#[test]
fn bounded_stream_reports_the_first_dropped_sequence() {
    let (recorder, mut stream) = bounded(1);
    let runtime = crate::identity::RuntimeId::next();
    recorder.record(
        runtime,
        RuntimeEventKind::ShutdownAdvanced {
            phase: crate::ShutdownPhase::Requested,
        },
    );
    recorder.record(
        runtime,
        RuntimeEventKind::ShutdownAdvanced {
            phase: crate::ShutdownPhase::JoiningCarriers,
        },
    );

    let status = stream.status();
    core::assert_eq!(status.capacity(), 1);
    core::assert_eq!(status.pending(), 1);
    core::assert_eq!(status.recorded(), 1);
    core::assert_eq!(status.dropped(), 1);
    core::assert_eq!(status.first_dropped().unwrap().get(), 1);
    core::assert!(!status.is_complete());
    core::assert_eq!(stream.drain()[0].sequence().get(), 0);
    core::assert_eq!(stream.status().pending(), 0);
}
