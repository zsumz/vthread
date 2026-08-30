use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use crate::TaskId;

use super::super::{WaitBegin, WaitCell, WaitHub, WakeCause};

#[test]
fn selected_timeout_emits_the_registered_task_identity() {
    let cell = WaitCell::new();
    let hub = Rc::new(WaitHub::new());
    let request = match cell
        .begin(
            TaskId::new(9),
            &hub,
            Some(Instant::now() + Duration::from_secs(1)),
        )
        .expect("begin wait")
    {
        WaitBegin::Park(request) => request,
        WaitBegin::Immediate(cause) => panic!("unexpected immediate wake: {cause:?}"),
    };
    let registration = hub
        .take_registration(request.token())
        .expect("registration");
    assert!(registration
        .select_timeout(request.token())
        .expect("timeout selection"));
    let notice = hub.pop_wake().expect("wake notice");
    assert_eq!(notice.task, TaskId::new(9));
    assert_eq!(notice.cause, WakeCause::TimedOut);
}
