use super::super::current;
use crate::{TaskId, wait::WaitHub, wait::WakeCause, wait::WakeNotice};
use std::sync::Arc;
use vthread_stack::ParkToken;

#[test]
fn matching_mounted_carrier_routes_and_unregisters_wakes_locally() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("local-wake", || {
                    let mounted = current().unwrap();
                    let execution = mounted.execution().unwrap();
                    let first = WakeNotice {
                        token: ParkToken::new(1, 1),
                        task: TaskId::new(1),
                        cause: WakeCause::Ready,
                    };
                    let second = WakeNotice {
                        token: ParkToken::new(2, 1),
                        task: TaskId::new(2),
                        cause: WakeCause::Closed,
                    };
                    assert!(super::enqueue_local_wake(execution.hub(), first));
                    assert!(super::enqueue_local_wake(execution.hub(), second));
                    super::unregister_local_wake(execution.hub(), first.token);
                    assert_eq!(execution.local().pop_wake(), Some(second));
                    assert_eq!(execution.local().pending_wakes(), 0);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn a_foreign_hub_is_never_routed_to_the_mounted_carrier() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("foreign-wake", || {
                    let foreign = Arc::new(WaitHub::new(1, Arc::default()));
                    let notice = WakeNotice {
                        token: ParkToken::new(1, 1),
                        task: TaskId::new(1),
                        cause: WakeCause::Ready,
                    };
                    assert!(!super::enqueue_local_wake(&foreign, notice));
                    let mounted = current().unwrap();
                    assert_eq!(mounted.execution().unwrap().local().pending_wakes(), 0);
                })?
                .join()
        })
        .unwrap();
}
