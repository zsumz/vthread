#[test]
fn disabled_recording_is_a_noop() {
    let shared = crate::control::Shared::new(crate::RuntimeConfig::default());
    shared.record(
        crate::diagnostics::evidence::RuntimeEventKind::ShutdownAdvanced {
            phase: crate::ShutdownPhase::Requested,
        },
    );
}
