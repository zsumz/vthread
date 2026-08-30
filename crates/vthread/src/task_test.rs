use std::sync::Arc;

use crate::task::TaskRecord;
use crate::{SuspensionReason, TaskId, TaskStatus, WakeReason};

#[test]
fn snapshots_copy_operator_visible_state() {
    let record = TaskRecord {
        id: TaskId::new(3),
        scope: 1,
        parent: None,
        options: crate::options::TaskOptions::root(crate::ScopeOptions::default(), 4),
        completion: Arc::new(crate::completion::Completion::new(4)),
        carrier: crate::CarrierId(0),
        deadline: None,
        failure: None,
        name: Arc::from("query"),
        status: TaskStatus::Suspended(SuspensionReason::Park),
        mounts: 2,
        yields: 1,
        parks: 1,
        last_suspension: Some(SuspensionReason::Park),
        last_wake: Some(WakeReason::Ready),
        outcome_observed: false,
        panic: None,
    };
    let snapshot = record.snapshot();

    assert_eq!(snapshot.id.to_string(), "3");
    assert_eq!(snapshot.name, "query");
    assert_eq!(snapshot.mounts, 2);
    assert_eq!(snapshot.yields, 1);
    assert_eq!(snapshot.parks, 1);
    assert_eq!(snapshot.last_suspension, Some(SuspensionReason::Park));
    assert_eq!(snapshot.last_wake, Some(WakeReason::Ready));
}
