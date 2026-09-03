use super::LocalCarrier;
use crate::{RuntimeConfig, TaskId, wait::WakeCause, wait::WakeNotice};
use vthread_stack::ParkToken;

#[test]
fn local_admission_starts_empty_without_allocating_stacks() {
    let carrier = LocalCarrier::new(RuntimeConfig::default());
    assert!(carrier.check_capacity().is_ok());
    assert_eq!(carrier.stacks.borrow().snapshot().allocated, 0);
}

#[test]
fn local_wakes_are_fifo_and_can_be_unregistered_exactly() {
    let carrier = LocalCarrier::new(RuntimeConfig::default());
    let first = WakeNotice {
        token: ParkToken::new(1, 1),
        task: TaskId::new(1),
        cause: WakeCause::Ready,
    };
    let second = WakeNotice {
        token: ParkToken::new(2, 1),
        task: TaskId::new(2),
        cause: WakeCause::Ready,
    };
    carrier.push_wake(first);
    carrier.push_wake(second);
    assert_eq!(carrier.pending_wakes(), 2);
    carrier.unregister_wake(first.token);
    assert_eq!(carrier.pending_wakes(), 1);
    assert_eq!(carrier.pop_wake(), Some(second));
    assert_eq!(carrier.pending_wakes(), 0);
}
