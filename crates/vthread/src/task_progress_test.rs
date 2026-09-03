use super::{COUNTER_BATCH, CarrierProgress, TaskProgress, TaskProgressWriter};
use crate::{SuspensionReason, TaskId, TaskStatus};

#[test]
fn progress_preserves_hot_mount_and_yield_observations() {
    let progress = TaskProgress::new();
    let carrier = CarrierProgress::new();
    let writer = TaskProgressWriter::new();
    let task = TaskId::new(1);
    assert!(!writer.resuming_yield());
    assert!(writer.mount(&carrier, task));
    assert_eq!(carrier.mounted(), Some(task));
    assert_eq!(
        progress.status(TaskStatus::Ready, true),
        TaskStatus::Running
    );
    assert_eq!(progress.mounts(), 0);

    writer.yield_now(&carrier, |update| progress.apply(update));
    assert!(writer.resuming_yield());
    assert_eq!(carrier.mounted(), None);
    assert_eq!(progress.status(TaskStatus::Ready, false), TaskStatus::Ready);
    assert_eq!(progress.yields(), 0);
    assert_eq!(
        progress.last_suspension(None),
        Some(SuspensionReason::YieldNow)
    );

    assert!(!writer.mount(&carrier, task));
    writer.park(&progress, &carrier);
    assert!(!writer.resuming_yield());
    assert_eq!(progress.mounts(), 2);
    assert_eq!(progress.yields(), 1);
    assert_eq!(
        progress.last_suspension(Some(SuspensionReason::Park)),
        Some(SuspensionReason::Park)
    );
}

#[test]
fn active_counter_lag_is_bounded_by_one_batch() {
    let progress = TaskProgress::new();
    let carrier = CarrierProgress::new();
    let writer = TaskProgressWriter::new();
    let task = TaskId::new(1);
    for _ in 1..COUNTER_BATCH {
        writer.mount(&carrier, task);
        writer.yield_now(&carrier, |update| progress.apply(update));
    }
    assert_eq!(progress.mounts(), 0);
    assert_eq!(progress.yields(), 0);

    writer.mount(&carrier, task);
    assert_eq!(progress.mounts(), 0);
    assert_eq!(progress.yields(), 0);
    writer.yield_now(&carrier, |update| progress.apply(update));
    assert_eq!(progress.mounts(), COUNTER_BATCH);
    assert_eq!(progress.yields(), COUNTER_BATCH);
}

#[test]
fn only_a_mounted_terminal_transition_counts_as_a_mount() {
    let progress = TaskProgress::new();
    let carrier = CarrierProgress::new();
    let writer = TaskProgressWriter::new();
    let task = TaskId::new(1);

    writer.unmount(&progress, &carrier, task);
    assert_eq!(progress.mounts(), 0);
    writer.mount(&carrier, task);
    writer.unmount(&progress, &carrier, task);
    assert_eq!(progress.mounts(), 1);
    writer.unmount(&progress, &carrier, task);
    assert_eq!(progress.mounts(), 1);
}
