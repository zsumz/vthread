use super::Wait;
use crate::{Error, Runtime, SuspensionReason, context};

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
                        assert_eq!(data.reason.get(), SuspensionReason::Condvar);
                    }
                    assert_eq!(data.reason.get(), SuspensionReason::Mutex);
                    drop(outer);
                    assert_eq!(data.reason.get(), SuspensionReason::Park);
                })?
                .join()
        })
        .unwrap();
}
