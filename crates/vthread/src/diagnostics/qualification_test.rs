use super::{GenerationSource, ProbeWakeResult, probe_pair};
use crate::diagnostics::evidence::{RuntimeEventKind, WakeRejection};

fn next(source: &GenerationSource) -> super::GenerationWake {
    for _ in 0..10_000 {
        if let Some(generation) = source.take() {
            return generation;
        }
        std::thread::yield_now();
    }
    core::panic!("probe generation was not published");
}

#[test]
fn retired_generation_is_rejected_by_the_real_selector() {
    let runtime = crate::Runtime::builder()
        .evidence_capacity(256)
        .build()
        .unwrap();
    let mut evidence = runtime.take_evidence().unwrap();
    runtime
        .run_scope(|scope| {
            let (parker, source) = probe_pair();
            let (started, progress) = std::sync::mpsc::sync_channel(0);
            let mut task = scope.spawn("stale-probe", move || {
                started.send(()).unwrap();
                parker.park().unwrap();
                started.send(()).unwrap();
                parker.park().unwrap();
            })?;
            progress.recv().unwrap();
            let first = next(&source);
            core::assert_eq!(first.offer_ready(), ProbeWakeResult::Selected);
            progress.recv().unwrap();
            let second = next(&source);
            core::assert_eq!(first.wait_key().wait(), second.wait_key().wait());
            core::assert!(first.wait_key().generation() < second.wait_key().generation());
            core::assert_eq!(first.offer_ready(), ProbeWakeResult::Rejected);
            core::assert_eq!(second.offer_ready(), ProbeWakeResult::Selected);
            task.join()?;
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();

    core::assert!(evidence.drain().iter().any(|event| core::matches!(
        event.kind(),
        RuntimeEventKind::WakeRejected {
            reason: WakeRejection::RetiredGeneration,
            ..
        }
    )));
    core::assert!(evidence.status().is_complete());
}
