use super::{COUNTER_BATCH, TaskProgress, TaskProgressWriter};
use crate::{SuspensionReason, TaskStatus};

#[test]
fn progress_preserves_hot_mount_and_yield_observations() {
    let progress = TaskProgress::new();
    let writer = TaskProgressWriter::new();
    assert!(writer.mount(&progress));
    assert_eq!(progress.status(TaskStatus::Ready), TaskStatus::Running);
    assert_eq!(progress.mounts(), 0);

    writer.yield_now(&progress);
    assert_eq!(progress.status(TaskStatus::Ready), TaskStatus::Ready);
    assert_eq!(progress.yields(), 0);
    assert_eq!(
        progress.last_suspension(None),
        Some(SuspensionReason::YieldNow)
    );

    writer.park(&progress);
    assert_eq!(progress.mounts(), 1);
    assert_eq!(progress.yields(), 1);
    assert_eq!(
        progress.last_suspension(Some(SuspensionReason::Park)),
        Some(SuspensionReason::Park)
    );
}

#[test]
fn active_counter_lag_is_bounded_by_one_batch() {
    let progress = TaskProgress::new();
    let writer = TaskProgressWriter::new();
    for _ in 1..COUNTER_BATCH {
        writer.mount(&progress);
        writer.yield_now(&progress);
    }
    assert_eq!(progress.mounts(), 0);
    assert_eq!(progress.yields(), 0);

    writer.mount(&progress);
    assert_eq!(progress.mounts(), 0);
    assert_eq!(progress.yields(), 0);
    writer.yield_now(&progress);
    assert_eq!(progress.mounts(), COUNTER_BATCH);
    assert_eq!(progress.yields(), COUNTER_BATCH);
}
