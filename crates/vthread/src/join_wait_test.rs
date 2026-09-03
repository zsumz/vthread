use crate::{Runtime, park_pair, support_test::until};

#[test]
fn a_virtual_join_parks_and_releases_the_single_carrier_for_other_work() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let (parker, waker) = park_pair();
            let mut first = scope.spawn("target", move || {
                parker.park().unwrap();
                42
            })?;
            let mut joining = scope.spawn("joiner", move || first.join())?;
            until(|| scope.runtime_snapshot().parked == 2);
            let mut ready = scope.spawn("other work", move || waker.unpark())?;
            ready.join()?;
            assert_eq!(joining.join()??, 42);
            Ok(())
        })
        .unwrap();
}

#[test]
fn self_join_is_typed_misuse_without_corrupting_completion() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            assert_eq!(
                scope
                    .spawn("self wait", || {
                        let mounted = crate::context::current().unwrap();
                        let execution = mounted.execution().unwrap();
                        let id = execution.record().lock().id;
                        assert!(matches!(
                            super::wait_for(
                                execution.record(),
                                crate::SuspensionReason::Join(id),
                                false
                            ),
                            Err(crate::Error::JoinSelf)
                        ));
                        42
                    })?
                    .join()?,
                42
            );
            Ok(())
        })
        .unwrap();
}
