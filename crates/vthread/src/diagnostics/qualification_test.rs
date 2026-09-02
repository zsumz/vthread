use super::{GenerationSource, ProbeWakeResult, probe_pair};
use crate::diagnostics::evidence::{RuntimeEventKind, WakeOrigin, WakeRejection};

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

#[test]
fn retired_generation_is_rejected_after_parker_moves_to_a_new_task() {
    let runtime = crate::Runtime::builder()
        .evidence_capacity(512)
        .build()
        .unwrap();
    let mut evidence = runtime.take_evidence().unwrap();
    let (first_task, second_task, first_wait, second_wait) = runtime
        .run_scope(|scope| {
            let (parker, source) = probe_pair();
            let (started, progress) = std::sync::mpsc::sync_channel(0);
            let mut task_a = scope.spawn("probe-a", move || {
                started.send(()).unwrap();
                parker.park().unwrap();
                parker
            })?;
            let task_a_id = task_a.task_id();
            progress.recv().unwrap();
            let first = next(&source);
            let first_wait = first.wait_key();
            core::assert_eq!(first.offer_ready(), ProbeWakeResult::Selected);
            let parker = task_a.join()?;

            let (started, progress) = std::sync::mpsc::sync_channel(0);
            let mut task_b = scope.spawn("probe-b", move || {
                started.send(()).unwrap();
                parker.park().unwrap();
            })?;
            let task_b_id = task_b.task_id();
            progress.recv().unwrap();
            let second = next(&source);
            let second_wait = second.wait_key();
            core::assert_eq!(first_wait.wait(), second_wait.wait());
            core::assert!(first_wait.generation() < second_wait.generation());
            core::assert_eq!(first.offer_ready(), ProbeWakeResult::Rejected);
            core::assert_eq!(second.offer_ready(), ProbeWakeResult::Selected);
            task_b.join()?;
            Ok((task_a_id, task_b_id, first_wait, second_wait))
        })
        .unwrap();
    runtime.shutdown().unwrap();

    core::assert!(first_task != second_task);
    let events = evidence.drain();
    core::assert!(events.iter().any(|event| core::matches!(
        event.kind(),
        RuntimeEventKind::WakeRejected {
            task,
            wait,
            origin: WakeOrigin::External,
            reason: WakeRejection::RetiredGeneration,
            ..
        } if task == first_task && wait == first_wait
    )));
    core::assert!(events.iter().any(|event| core::matches!(
        event.kind(),
        RuntimeEventKind::WakeSelected {
            task,
            wait,
            origin: WakeOrigin::External,
            ..
        }
            if task == second_task && wait == second_wait
    )));
    core::assert!(evidence.status().is_complete());
}
