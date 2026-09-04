use std::{sync::Arc, time::Instant};

use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WaitBegin, WaitCell, WaitHub, WakeCause},
};

#[test]
fn an_expired_deadline_does_not_activate_a_wait_generation() {
    let wait = WaitCell::new();
    let hub = Arc::new(WaitHub::new(1, Arc::default()));

    assert!(matches!(
        wait.begin(
            TaskId::new(1),
            TaskKey::owned(0),
            &hub,
            Some(Instant::now())
        ),
        Ok(WaitBegin::Immediate(WakeCause::TimedOut))
    ));
    assert!(!hub.has_pending());
}
