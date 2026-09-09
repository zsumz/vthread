use super::Wait;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use crate::{Error, Runtime, SuspensionReason, context};

const UNSET: u8 = 0;
const REJECTED: u8 = 1;
const ENTERED: u8 = 2;
const UNEXPECTED: u8 = 3;

struct EnterWaitOnDrop(Arc<AtomicU8>);

impl Drop for EnterWaitOnDrop {
    fn drop(&mut self) {
        let outcome = match Wait::enter_after_check(SuspensionReason::Mutex) {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(_) => ENTERED,
            Err(_) => UNEXPECTED,
        };
        self.0.store(outcome, Ordering::SeqCst);
    }
}

#[test]
fn diagnostic_reason_is_nested_and_restored() {
    assert!(matches!(
        Wait::enter(SuspensionReason::Mutex),
        Err(Error::OutsideVThread)
    ));
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("reasons", || {
                    let mounted = context::current().unwrap();
                    let data = &mounted.execution().unwrap().data;
                    let outer = Wait::enter(SuspensionReason::Mutex).unwrap();
                    {
                        let _inner = Wait::enter(SuspensionReason::Condvar).unwrap();
                        assert_eq!(data.reason(), SuspensionReason::Condvar);
                    }
                    assert_eq!(data.reason(), SuspensionReason::Mutex);
                    drop(outer);
                    assert_eq!(data.reason(), SuspensionReason::Park);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn panic_is_rejected_before_a_synchronization_wait_changes_task_state() {
    let outcome = Arc::new(AtomicU8::new(UNSET));
    let task_outcome = Arc::clone(&outcome);
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            let mut task = scope.spawn("panic wait", move || {
                let _wait = EnterWaitOnDrop(task_outcome);
                panic!("expected synchronization panic");
            })?;
            assert!(matches!(task.join(), Err(Error::TaskPanicked { .. })));
            Ok(())
        })
        .unwrap();

    assert_eq!(outcome.load(Ordering::SeqCst), REJECTED);
}
