use super::Shared;
use crate::{Runtime, StallPolicy, TaskStatus, signal::lock};
use std::time::Duration;

#[test]
fn transition_without_stall_detection_does_not_publish_control_activity() {
    let shared = Shared::new(Runtime::builder().build().unwrap().config());
    let scope = shared.begin_scope().unwrap();
    let record = shared
        .reserve(scope, "quiet transition".into(), None)
        .unwrap();
    let observed = shared.changed.version();

    shared.transition(&record, |task| task.status = TaskStatus::Running);

    assert_eq!(shared.changed.version(), observed);
    shared.complete(&record, None);
    shared.finish_scope(scope);
}

#[test]
fn transition_notifies_and_counts_enabled_stall_detection() {
    let config = Runtime::builder()
        .stall_policy(StallPolicy::ReportAfter(Duration::from_secs(1)))
        .build()
        .unwrap()
        .config();
    let shared = Shared::new(config);
    let scope = shared.begin_scope().unwrap();
    let record = shared
        .reserve(scope, "observed transition".into(), None)
        .unwrap();
    let observed_signal = shared.changed.version();
    let observed_activity = lock(&shared.state).scopes[&scope].progress.activity();

    shared.transition(&record, |task| task.status = TaskStatus::Running);

    assert!(shared.changed.version() > observed_signal);
    assert_eq!(
        lock(&shared.state).scopes[&scope].progress.activity(),
        observed_activity + 1
    );
    shared.complete(&record, None);
    shared.finish_scope(scope);
}
