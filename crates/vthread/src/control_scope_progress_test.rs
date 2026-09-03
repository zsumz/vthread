use super::ScopeProgress;
use crate::TaskId;

#[test]
fn admission_and_batched_retirement_form_one_scope_snapshot() {
    let progress = ScopeProgress::new();
    progress.publish_admitted(3, false);
    assert_eq!(progress.active(), 3);

    assert!(!progress.retire(2, 1, 1, 0, &[TaskId::new(2)], false));
    let partial = progress.snapshot();
    assert_eq!(progress.active(), 1);
    assert_eq!(partial.completed, 1);
    assert_eq!(partial.panicked, 1);
    assert_eq!(progress.failed_tasks(), vec![TaskId::new(2)]);

    assert!(progress.retire(1, 0, 0, 1, &[TaskId::new(3)], true));
    let complete = progress.snapshot();
    assert_eq!(progress.active(), 0);
    assert_eq!(complete.aborted, 1);
    assert_eq!(progress.activity(), 1);
}
