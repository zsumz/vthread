use std::{cell::RefCell, rc::Rc};

use crate::task::TaskRecord;
use crate::{SuspensionReason, TaskId, TaskStatus};

#[test]
fn snapshots_copy_operator_visible_state() {
    let record = TaskRecord {
        id: TaskId::new(3),
        scope: 1,
        name: Rc::from("query"),
        status: TaskStatus::Suspended(SuspensionReason::YieldNow),
        mounts: 2,
        yields: 1,
        last_suspension: Some(SuspensionReason::YieldNow),
        outcome_observed: false,
        panic: None,
    };
    let snapshot = Rc::new(RefCell::new(record)).borrow().snapshot();

    assert_eq!(snapshot.id.to_string(), "3");
    assert_eq!(snapshot.name, "query");
    assert_eq!(snapshot.mounts, 2);
    assert_eq!(snapshot.yields, 1);
    assert_eq!(snapshot.last_suspension, Some(SuspensionReason::YieldNow));
}
