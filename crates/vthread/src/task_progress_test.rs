use super::TaskProgress;
use crate::{SuspensionReason, TaskStatus};

#[test]
fn progress_preserves_hot_mount_and_yield_observations() {
    let progress = TaskProgress::new();
    assert!(progress.mount());
    assert_eq!(progress.status(TaskStatus::Ready), TaskStatus::Running);
    assert_eq!(progress.mounts(), 1);

    progress.yield_now();
    assert_eq!(progress.status(TaskStatus::Ready), TaskStatus::Ready);
    assert_eq!(progress.yields(), 1);
    assert_eq!(
        progress.last_suspension(None),
        Some(SuspensionReason::YieldNow)
    );

    progress.clear_yield();
    assert_eq!(
        progress.last_suspension(Some(SuspensionReason::Park)),
        Some(SuspensionReason::Park)
    );
}
