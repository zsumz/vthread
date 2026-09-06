use crate::{Error, Runtime, park_pair};

#[test]
fn an_unselected_registration_error_leaves_no_live_wait() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let mut child = scope.spawn("registration error", || {
                let (parker, waker) = park_pair();
                let result = parker.park_registered(|_, _| Err::<(), _>(Error::WouldBlock));
                assert!(matches!(result, Err(Error::WouldBlock)));
                assert_eq!(waker.unpark(), crate::UnparkResult::Stored);
                assert_eq!(parker.park().unwrap(), crate::ParkOutcome::Ready);
            })?;
            child.join()?;
            Ok(())
        })
        .unwrap();
}
