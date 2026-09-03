use super::Recorder;
use std::time::Duration;

#[test]
fn cumulative_profile_returns_checked_phase_deltas() {
    let recorder = Recorder::new();
    let before = recorder.snapshot();
    recorder.record_admission(
        Duration::from_nanos(13),
        Duration::from_nanos(3),
        Duration::from_nanos(2),
    );
    recorder.record_stack_fiber(Duration::from_nanos(11));
    recorder.record_reclaim(Duration::from_nanos(7));
    recorder.record_completion(Duration::from_nanos(5), 1);

    let delta = recorder.snapshot().checked_delta(before).unwrap();
    core::assert_eq!(delta.reservation_nanoseconds(), 13);
    core::assert_eq!(delta.envelope_nanoseconds(), 3);
    core::assert_eq!(delta.inbox_nanoseconds(), 2);
    core::assert_eq!(delta.admission_operations(), 1);
    core::assert_eq!(delta.stack_fiber_nanoseconds(), 11);
    core::assert_eq!(delta.stack_fiber_operations(), 1);
    core::assert_eq!(delta.reclaim_nanoseconds(), 7);
    core::assert_eq!(delta.reclaim_operations(), 1);
    core::assert_eq!(delta.completion_nanoseconds(), 5);
    core::assert_eq!(delta.completion_operations(), 1);
    core::assert_eq!(
        before
            .checked_delta(before)
            .unwrap()
            .completion_operations(),
        0
    );
}

#[test]
fn runtime_profile_covers_one_owned_task_lifecycle() {
    let runtime = crate::Runtime::new().unwrap();
    let before = runtime.lifecycle_profile();
    runtime
        .run_scope(|scope| {
            drop(scope.spawn("profiled", || ())?);
            Ok(())
        })
        .unwrap();
    let delta = runtime.lifecycle_profile().checked_delta(before).unwrap();

    core::assert_eq!(delta.admission_operations(), 1);
    core::assert_eq!(delta.stack_fiber_operations(), 1);
    core::assert_eq!(delta.reclaim_operations(), 1);
    core::assert_eq!(delta.completion_operations(), 1);
}
