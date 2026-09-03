use super::CancellationToken;

#[test]
fn cancellation_flows_down_but_never_up_or_across_siblings() {
    let parent = CancellationToken::root(2);
    let left = parent.child_token();
    let right = parent.child_token();
    left.cancel();
    assert!(left.is_cancelled());
    assert!(!parent.is_cancelled());
    assert!(!right.is_cancelled());
    parent.cancel();
    assert!(right.child_token().is_cancelled());
}

#[test]
fn leaf_retirement_is_batched_with_a_fixed_residual_bound() {
    let parent = CancellationToken::root(64);
    let children = (0..63).map(|_| parent.child_token()).collect::<Vec<_>>();
    drop(children);
    assert_eq!(parent.pending_retirements(), 63);

    drop(parent.child_token());
    assert_eq!(parent.pending_retirements(), 0);
    assert_eq!(parent.graph_snapshot(), (1, 0, 0));
}

#[test]
fn residual_retirements_are_destroyed_with_the_domain() {
    let parent = CancellationToken::root(64);
    let domain = std::sync::Arc::downgrade(&parent.0.domain);
    drop(parent.child_token());
    assert_eq!(parent.pending_retirements(), 1);

    drop(parent);
    assert!(domain.upgrade().is_none());
}

#[test]
fn inherited_cancellation_wakes_children_and_drains_borrowed_stacks() {
    use crate::{Error, Runtime, local_scope, park_pair, support_test::until};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Runtime::new().unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    runtime
        .run_scope(|scope| {
            let tracked = Arc::clone(&drops);
            let mut parent = scope.spawn("parent", move || {
                local_scope(|local| {
                    for _ in 0..2 {
                        let guard = Guard(Arc::clone(&tracked));
                        let _ = local.spawn("child", move || {
                            let _guard = guard;
                            let (parker, _waker) = park_pair();
                            parker.park()
                        })?;
                    }
                    Ok(())
                })
            })?;
            until(|| scope.runtime_snapshot().parked == 3);
            scope.cancel();
            assert!(matches!(
                parent.join()?.as_ref().map_err(crate::Error::primary),
                Err(Error::Cancelled)
            ));
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            assert_eq!(scope.runtime_snapshot().active, 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn inherited_deadline_interrupts_a_long_sleep() {
    use crate::{Error, Runtime, ScopeOptions};
    use std::time::{Duration, Instant};
    let runtime = Runtime::new().unwrap();
    let deadline = Instant::now() + Duration::from_millis(30);
    let outcome = runtime.run_scope_with(ScopeOptions::default().deadline(deadline), |scope| {
        scope
            .spawn("deadline", || crate::sleep(Duration::from_secs(5)))?
            .join()?
    });
    assert!(matches!(
        outcome.as_ref().map_err(crate::Error::primary),
        Err(Error::DeadlineExceeded)
    ));
    assert_eq!(runtime.snapshot().active, 0);
}

#[test]
fn cancellation_racing_registration_does_not_lose_the_request() {
    use crate::{Error, Runtime, park_pair};
    use std::{sync::mpsc, thread, time::Duration};
    let runtime = Runtime::new().unwrap();
    for _ in 0..64 {
        runtime
            .run_scope(|scope| {
                let (tx, rx) = mpsc::sync_channel(1);
                let token = scope.cancellation_token();
                let remote = thread::spawn(move || {
                    rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    token.cancel();
                });
                let mut child = scope.spawn("race", move || {
                    let (parker, _waker) = park_pair();
                    tx.send(()).unwrap();
                    parker.park_timeout(Duration::from_secs(5))
                })?;
                assert!(matches!(child.join()?, Err(Error::Cancelled)));
                remote.join().unwrap();
                Ok(())
            })
            .unwrap();
    }
}
