use super::SchedulerProfile;
use crate::{CarrierId, CarrierStatus, Runtime, TaskFailure, control::Shared, kernel::Kernel};
use std::sync::Arc;

#[test]
fn receive_bins_distinguish_empty_drains_from_window_deferrals() {
    let mut profile = SchedulerProfile::default();
    let sizes = [0, 1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 128];
    for size in sizes {
        profile.record_receive(Some(size));
    }
    profile.record_receive(None);
    core::assert_eq!(profile.receive_calls(), 15);
    core::assert_eq!(profile.window_deferrals(), 1);
    core::assert_eq!(profile.batch_histogram(), [1, 1, 2, 2, 2, 2, 2, 2]);
    core::assert_eq!(profile.remote_packets(), sizes.iter().sum::<usize>() as u64);
}

#[test]
fn idle_counts_separate_progress_polling_and_wait_calls() {
    let mut profile = SchedulerProfile::default();
    for mounts in [0, 4, 4, 12] {
        profile.record_idle(mounts);
    }
    profile.record_early_work();
    profile.record_poll(3, true);
    profile.record_poll(640, false);
    profile.record_wait(false);
    profile.record_wait_return(true);
    profile.record_wait(true);
    profile.record_wait_return(false);
    core::assert_eq!(profile.idle_entries(), 4);
    core::assert_eq!(profile.idle_dispatches(), 12);
    core::assert_eq!(profile.maximum_idle_dispatches(), 8);
    core::assert_eq!(profile.empty_idle_entries(), 2);
    core::assert_eq!(profile.early_work_returns(), 1);
    core::assert_eq!(profile.poll_episodes(), 2);
    core::assert_eq!(profile.poll_probes(), 643);
    core::assert_eq!(profile.poll_hits(), 1);
    core::assert_eq!(profile.wait_calls(), 2);
    core::assert_eq!(profile.timed_wait_calls(), 1);
    core::assert_eq!(profile.returns_without_task_work(), 1);
}

fn shared(carriers: usize, capacity: usize) -> Arc<Shared> {
    let config = Runtime::builder()
        .carriers(carriers)
        .max_vthreads(capacity)
        .carrier_queue_capacity(capacity)
        .stack_cache_capacity(capacity)
        .build()
        .unwrap()
        .config();
    Arc::new(Shared::new(config))
}

#[test]
fn real_receive_counts_and_publishes_bounded_batches() {
    let shared = shared(1, 65);
    let scope = shared.begin_scope().unwrap();
    for _ in 0..65 {
        shared.submit(scope, "queued".into(), || ()).unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    core::assert!(kernel.receive());
    core::assert!(kernel.receive());
    core::assert_eq!(kernel.scheduler_profile.remote_packets(), 64);
    core::assert_eq!(kernel.scheduler_profile.window_deferrals(), 1);
    for _ in 0..32 {
        core::assert!(kernel.tick(false).unwrap());
    }
    core::assert!(!kernel.receive());
    let profile = kernel.scheduler_profile;
    core::assert_eq!(profile.receive_calls(), 3);
    core::assert_eq!(profile.remote_packets(), 65);
    core::assert_eq!(profile.batch_histogram(), [0, 1, 0, 0, 0, 0, 0, 1]);
    kernel.publish(CarrierStatus::Running);
    core::assert_eq!(shared.snapshot().carriers()[0].scheduler_profile(), profile);
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}

#[test]
fn a_changed_epoch_records_one_poll_probe_without_a_wait() {
    let shared = shared(2, 2);
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    let observed = kernel.inbox.signal.version();
    kernel.inbox.signal.notify();
    kernel.wait_for_work(observed);
    let profile = kernel.scheduler_profile;
    core::assert_eq!(profile.idle_entries(), 1);
    core::assert_eq!(profile.empty_idle_entries(), 1);
    core::assert_eq!(profile.poll_episodes(), 1);
    core::assert_eq!(profile.poll_probes(), 1);
    core::assert_eq!(profile.poll_hits(), 1);
    core::assert_eq!(profile.wait_calls(), 0);
}

#[test]
fn a_due_timer_records_a_wait_call_without_claiming_a_native_sleep() {
    use crate::task_slab::TaskKey;
    use std::time::Instant;
    use vthread_stack::ParkToken;

    let shared = shared(2, 2);
    let mut kernel = Kernel::new(shared, CarrierId(0));
    let token = ParkToken::new(1, 1);
    core::assert!(
        kernel
            .timers
            .schedule(TaskKey::owned(0), token, Instant::now())
    );
    kernel.wait_for_work(kernel.inbox.signal.version());
    let profile = kernel.scheduler_profile;
    core::assert_eq!(profile.poll_episodes(), 0);
    core::assert_eq!(profile.wait_calls(), 1);
    core::assert_eq!(profile.timed_wait_calls(), 1);
    core::assert_eq!(profile.returns_without_task_work(), 1);
    core::assert!(kernel.timers.cancel(token));
}
