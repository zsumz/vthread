use super::{EvidenceCapabilities, bounded};

#[test]
fn capabilities_include_exact_wait_and_stack_evidence() {
    let (_, stream) = bounded(1);
    let capabilities = stream.capabilities();
    core::assert!(
        capabilities
            .contains(EvidenceCapabilities::TOTAL_ORDER | EvidenceCapabilities::WAIT_GENERATIONS)
    );
    core::assert!(capabilities.contains(EvidenceCapabilities::STACK_IDENTITIES));
    core::assert!(capabilities.contains(EvidenceCapabilities::OWNED_SCOPE_LIFECYCLE));
    core::assert!(capabilities.contains(EvidenceCapabilities::WAKE_ORIGINS));
    core::assert_eq!(stream.schema_version(), 1);
}

#[test]
fn runtime_stream_covers_one_complete_timed_task_lifetime() {
    use super::{EvidenceWakeCause, RuntimeEventKind, TaskOutcome, WakeOrigin};

    let runtime = crate::Runtime::builder()
        .evidence_capacity(1024)
        .build()
        .unwrap();
    let mut stream = runtime.take_evidence().unwrap();
    let task = runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("evidence", || {
                crate::yield_now().unwrap();
                crate::sleep(std::time::Duration::from_millis(1)).unwrap();
            })?;
            let id = task.task_id();
            task.join()?;
            Ok(id)
        })
        .unwrap();
    runtime.shutdown().unwrap();

    let events = stream.drain();
    for (expected, event) in events.iter().enumerate() {
        core::assert_eq!(event.sequence().get(), expected as u64);
    }
    let kinds = events.iter().map(|event| event.kind()).collect::<Vec<_>>();
    core::assert!(kinds.iter().any(|kind| core::matches!(
        kind,
        RuntimeEventKind::TaskAccepted { task: id, .. } if *id == task
    )));
    core::assert!(kinds.iter().any(|kind| core::matches!(
        kind,
        RuntimeEventKind::StackCheckedOut { task: id, .. } if *id == task
    )));
    core::assert!(kinds.iter().any(|kind| core::matches!(
        kind,
        RuntimeEventKind::WakeSelected {
            task: id,
            cause: EvidenceWakeCause::TimedOut,
            origin: WakeOrigin::Carrier(_),
            ..
        } if *id == task
    )));
    core::assert!(kinds.iter().any(|kind| core::matches!(
        kind,
        RuntimeEventKind::TaskTerminated {
            task: id,
            outcome: TaskOutcome::Completed,
        } if *id == task
    )));
    core::assert!(kinds.iter().any(|kind| core::matches!(
        kind,
        RuntimeEventKind::ShutdownAdvanced {
            phase: crate::ShutdownPhase::Complete,
        }
    )));
    core::assert!(stream.status().is_complete());
    core::assert!(stream.status().runtime_terminal());
}
