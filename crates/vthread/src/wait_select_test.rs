use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::TaskId;

use super::super::{WaitBegin, WaitCell, WaitHub, WakeCause};

#[test]
fn selected_timeout_emits_the_registered_task_identity() {
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
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
    assert!(
        registration
            .select_timeout(request.token())
            .expect("timeout selection")
    );
    let notice = hub.pop_wake().expect("wake notice");
    assert_eq!(notice.task, TaskId::new(9));
    assert_eq!(notice.cause, WakeCause::TimedOut);
}

#[test]
fn concurrent_ready_timeout_cancel_and_close_select_exactly_one_notice() {
    use std::{sync::Barrier, thread};
    for _ in 0..64 {
        let cell = WaitCell::new();
        let hub = Arc::new(WaitHub::new(1, Arc::default()));
        let WaitBegin::Park(request) = cell.begin(TaskId::new(9), &hub, None).expect("park") else {
            panic!("expected a park");
        };
        let token = request.token();
        let registration = hub.take_registration(token).expect("registration");
        let barrier = Barrier::new(4);
        thread::scope(|threads| {
            threads.spawn(|| {
                barrier.wait();
                cell.notify();
            });
            threads.spawn(|| {
                barrier.wait();
                cell.cancel();
            });
            threads.spawn(|| {
                barrier.wait();
                cell.close();
            });
            threads.spawn(|| {
                barrier.wait();
                registration.select_timeout(token).expect("timeout");
            });
        });
        assert_eq!(hub.pending(), 1);
        let notice = hub.pop_wake().expect("one winner");
        assert_eq!(notice.token, token);
        assert_eq!(notice.task, TaskId::new(9));
        assert_eq!(cell.finish(token).expect("finish"), notice.cause);
        assert!(hub.pop_wake().is_none());
        assert_eq!(hub.stale(), 0);
    }
}
