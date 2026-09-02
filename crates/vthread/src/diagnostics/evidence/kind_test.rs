use super::{EvidenceWakeCause, RuntimeEventKind};

#[test]
fn evidence_kinds_are_copyable_values() {
    let kind = RuntimeEventKind::ShutdownAdvanced {
        phase: crate::ShutdownPhase::Requested,
    };
    core::assert_eq!(kind, kind);
    core::assert_eq!(EvidenceWakeCause::Ready, EvidenceWakeCause::Ready);
}
