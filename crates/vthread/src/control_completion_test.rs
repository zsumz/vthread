use crate::{RuntimeConfig, TaskStatus, control::Shared, signal::lock};

#[test]
fn completion_is_committed_once() {
    let shared = Shared::new(RuntimeConfig::default());
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "done".into(), None).unwrap();
    shared.complete(&record, None);
    shared.complete(&record, None);
    assert_eq!(lock(&record).status, TaskStatus::Completed);
}
