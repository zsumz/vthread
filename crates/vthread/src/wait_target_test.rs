use std::sync::Arc;

use crate::{TaskId, task_slab::TaskKey, wait::WaitHub};

use super::WaitInner;

#[test]
fn primary_target_is_reused_and_an_alternate_remains_explicit() {
    let primary = Arc::new(WaitHub::new(1, Arc::default()));
    let alternate = Arc::new(WaitHub::new(1, Arc::default()));
    let target = WaitInner::new(1);
    assert!(!target.bind_target(TaskId::new(1), TaskKey::owned(0), &primary));
    assert!(!target.bind_target(TaskId::new(2), TaskKey::owned(1), &primary));
    assert!(target.bind_target(TaskId::new(3), TaskKey::borrowed(2), &alternate));
}
