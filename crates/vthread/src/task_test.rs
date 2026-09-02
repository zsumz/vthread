use std::sync::Arc;

use crate::{SuspensionReason, TaskId, TaskStatus, WakeReason};
use crate::{
    task::{TaskCell, TaskRecord},
    task_progress::{CarrierProgress, TaskProgressWriter},
};

#[test]
fn snapshots_copy_operator_visible_state() {
    let task = TaskCell::new(
        TaskRecord {
            id: TaskId::new(3),
            scope: 1,
            parent: None,
            options: crate::options::TaskOptions::root(crate::ScopeOptions::default(), 4),
            carrier: crate::CarrierId(0),
            deadline: None,
            failure: None,
            name: Arc::from("query"),
            status: TaskStatus::Suspended(SuspensionReason::Park),
            parks: 1,
            last_suspension: Some(SuspensionReason::Park),
            last_wake: Some(WakeReason::Ready),
            outcome_observed: false,
            panic: None,
        },
        4,
    );
    let carrier = CarrierProgress::new();
    let writer = TaskProgressWriter::new();
    assert!(writer.mount(&carrier, TaskId::new(3)));
    writer.yield_now(task.progress(), &carrier);
    assert!(!writer.mount(&carrier, TaskId::new(3)));
    writer.park(task.progress(), &carrier);
    let snapshot = task.snapshot(&[carrier.mounted()]);

    assert_eq!(snapshot.id.to_string(), "3");
    assert_eq!(snapshot.name, "query");
    assert_eq!(snapshot.mounts, 2);
    assert_eq!(snapshot.yields, 1);
    assert_eq!(snapshot.parks, 1);
    assert_eq!(snapshot.last_suspension, Some(SuspensionReason::Park));
    assert_eq!(snapshot.last_wake, Some(WakeReason::Ready));
}
