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
        .scope(|scope| {
            let tracked = Arc::clone(&drops);
            let parent = scope.spawn("parent", move || {
                local_scope(|local| {
                    for _ in 0..2 {
                        let guard = Guard(Arc::clone(&tracked));
                        local.spawn("child", move || {
                            let _guard = guard;
                            let (parker, _waker) = park_pair();
                            parker.park()
                        })?;
                    }
                    Ok(())
                })
            })?;
            until(|| scope.snapshot().parked == 3);
            scope.cancel();
            assert!(matches!(parent.join()?, Err(Error::Cancelled)));
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            assert_eq!(scope.snapshot().active, 0);
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
    let outcome = runtime.scope_with(ScopeOptions::default().deadline(deadline), |scope| {
        scope
            .spawn("deadline", || crate::sleep(Duration::from_secs(5)))?
            .join()?
    });
    assert!(matches!(outcome, Err(Error::DeadlineExceeded)));
    assert_eq!(runtime.snapshot().active, 0);
}

#[test]
fn cancellation_racing_registration_does_not_lose_the_request() {
    use crate::{Error, Runtime, park_pair};
    use std::{sync::mpsc, thread, time::Duration};
    let runtime = Runtime::new().unwrap();
    for _ in 0..64 {
        runtime
            .scope(|scope| {
                let (tx, rx) = mpsc::sync_channel(1);
                let token = scope.cancellation_token();
                let remote = thread::spawn(move || {
                    rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    token.cancel();
                });
                let child = scope.spawn("race", move || {
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
