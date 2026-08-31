use crate::{Error, Runtime, SpawnOptions, park_pair, support_test::until};
use std::time::{Duration, Instant};

#[test]
fn handle_cancellation_wakes_only_its_child_and_preserves_result_observation() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    runtime
        .run_scope(|scope| {
            let mut child = scope.spawn("cancel me", || {
                let (park, _wake) = park_pair();
                park.park()
            })?;
            let mut sibling = scope.spawn("keep me", || 42)?;
            until(|| runtime.snapshot().parked() == 1);
            let token = child.cancellation_token();
            std::thread::spawn(move || token.cancel()).join().unwrap();
            assert!(matches!(child.join()?, Err(Error::Cancelled)));
            assert_eq!(sibling.join()?, 42);
            assert!(!scope.cancellation_token().is_cancelled());
            assert!(!sibling.cancellation_token().is_cancelled());
            child.cancel();
            child.wait()?;
            assert!(matches!(
                child.take_result(),
                Err(Error::ResultAlreadyTaken)
            ));
            Ok(())
        })
        .unwrap();
}

#[test]
fn local_child_can_be_cancelled_without_cancelling_its_group_or_sibling() {
    crate::run(|scope| {
        scope
            .spawn("parent", || {
                crate::local_scope(|local| {
                    let mut child =
                        local.spawn("cancel local", || crate::sleep(Duration::from_secs(5)))?;
                    let mut sibling = local.spawn("local sibling", || 7)?;
                    crate::yield_now()?;
                    child.cancel();
                    assert!(child.cancellation_token().is_cancelled());
                    assert!(!local.cancellation_token().is_cancelled());
                    assert!(matches!(child.join()?, Err(Error::Cancelled)));
                    child.wait()?;
                    assert_eq!(sibling.join()?, 7);
                    crate::checkpoint()
                })
            })?
            .join()??;
        Ok(())
    })
    .unwrap();
}

#[test]
fn child_deadline_interrupts_waiting_without_expiring_its_owner() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let options =
                SpawnOptions::default().deadline(Instant::now() + Duration::from_millis(200));
            let mut child =
                scope.spawn_with(options, "deadline", || crate::sleep(Duration::from_secs(5)))?;
            assert!(matches!(child.join()?, Err(Error::DeadlineExceeded)));
            assert_eq!(scope.deadline(), None);
            assert_eq!(scope.spawn("still open", || 42)?.join()?, 42);
            Ok(())
        })
        .unwrap();
}

#[test]
fn local_child_deadline_is_independent_and_an_expired_admission_is_rejected() {
    crate::run(|scope| {
        scope
            .spawn("parent", || {
                crate::local_scope(|local| {
                    let options = SpawnOptions::default()
                        .deadline(Instant::now() + Duration::from_millis(200));
                    let mut child = local
                        .spawn_with(options, "deadline", || crate::sleep(Duration::from_secs(5)))?;
                    assert!(matches!(child.join()?, Err(Error::DeadlineExceeded)));
                    assert_eq!(local.deadline(), None);
                    assert!(matches!(
                        local.spawn_with(
                            SpawnOptions::default().deadline(Instant::now()),
                            "expired",
                            || ()
                        ),
                        Err(Error::DeadlineExceeded)
                    ));
                    assert_eq!(local.spawn("sibling", || 7)?.join()?, 7);
                    Ok(())
                })
            })?
            .join()??;
        assert!(matches!(
            scope.spawn_with(
                SpawnOptions::default().deadline(Instant::now()),
                "expired",
                || ()
            ),
            Err(Error::DeadlineExceeded)
        ));
        Ok(())
    })
    .unwrap();
}
