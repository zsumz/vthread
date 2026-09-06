//! Distinct runtime ownership with controlled native-stack selection and recovery.

use super::kernel_policy_test::{Owner, wait_until};
use crate::{Error, SuspensionReason, wait::Publication};
use std::{io::Write, time::Duration};

#[test]
fn an_interrupted_cross_runtime_join_retains_its_handle_and_result() {
    let mut target_owner = Owner::new(None);
    let (gate, release) = crate::park_pair();
    let mut target = target_owner.submit("foreign pending target", move || {
        gate.park().unwrap();
        42
    });
    let target_id = target.task_id();
    assert!(matches!(target.take_result(), Err(Error::WouldBlock)));
    assert!(!target_owner.kernel.receive());
    assert!(target_owner.tick());
    assert_eq!(target_owner.kernel.parked.len(), 1);

    let mut joining_owner = Owner::new(Some(Duration::from_secs(1)));
    let target_runtime = target_owner.kernel.shared.id;
    let joining_runtime = joining_owner.kernel.shared.id;
    assert_ne!(target_runtime, joining_runtime);
    let deadline = joining_owner.deadline.unwrap();
    let mut waiter = joining_owner.submit("interruptible foreign join", move || {
        let result = target.join();
        (result, target)
    });
    assert_eq!(
        waiter.task_id(),
        target_id,
        "colliding IDs belong to distinct runtimes"
    );
    assert!(!joining_owner.kernel.receive());
    assert!(joining_owner.tick());
    let parked = joining_owner
        .kernel
        .parked
        .iter()
        .next()
        .unwrap_or_else(|| {
            panic!("foreign join never parked: {:?}", waiter.take_result());
        });
    let (route, token) = (parked.task, parked.token);
    let registration = parked.registration.as_ref().unwrap().clone();
    assert_eq!(
        joining_owner.kernel.task(route).execution().data.reason(),
        SuspensionReason::Join(target_id)
    );
    assert_eq!(joining_owner.kernel.timers.next_deadline(), Some(deadline));
    wait_until(deadline);
    joining_owner.kernel.expire_timers().unwrap();
    assert_eq!(registration.publication(token), Publication::Published);
    assert!(!registration.select_ready(token));
    assert!(joining_owner.kernel.ready.is_empty());
    assert!(!waiter.is_finished());
    assert_eq!(target_owner.kernel.parked.len(), 1);
    joining_owner.drain();

    let returned = waiter.take_result();
    writeln!(
        std::io::stdout().lock(),
        "cross-join target_runtime={target_runtime:?} joining_runtime={joining_runtime:?} \
         route={route:?} token={token:?} publication=Published returned={returned:?}"
    )
    .unwrap();
    let (interrupted, mut target) = returned.expect("preserve waiter panic or typed handle return");
    assert!(
        matches!(interrupted, Err(Error::DeadlineExceeded)),
        "{interrupted:?}"
    );
    assert_eq!(registration.publication(token), Publication::Stale);
    assert!(!target.is_finished());
    let unfinished = target.take_result();
    assert!(
        matches!(unfinished, Err(Error::WouldBlock)),
        "retained target: {unfinished:?}"
    );

    release.unpark();
    target_owner.drain();
    target.wait().unwrap();
    let completed = target.take_result();
    let second = target.join();
    writeln!(
        std::io::stdout().lock(),
        "cross-join recovery completed={completed:?} repeated={second:?}"
    )
    .unwrap();
    assert_eq!(completed.unwrap(), 42);
    assert!(matches!(second, Err(Error::ResultAlreadyTaken)));
    for owner in [&target_owner, &joining_owner] {
        assert_eq!(owner.kernel.shared.snapshot().active, 0);
        assert_eq!(owner.kernel.timers.active_count(), 0);
        owner.kernel.shared.wait(owner.scope, None).unwrap();
    }
    assert!(matches!(
        joining_owner
            .kernel
            .shared
            .scope_options(joining_owner.scope)
            .unwrap()
            .check(),
        Err(Error::DeadlineExceeded)
    ));
    assert_eq!(
        (
            joining_owner.kernel.stats.parks,
            joining_owner.kernel.stats.timeouts
        ),
        (1, 1)
    );
}

#[test]
fn a_completed_target_panic_is_not_a_join_deadline_failure() {
    let mut target_owner = Owner::new(None);
    let target = target_owner.submit("failed fixture target", || -> usize {
        panic!("ordered target gate failure");
    });
    let target_id = target.task_id();
    assert!(!target_owner.kernel.receive());
    target_owner.drain();
    assert!(target.is_finished());
    let mut joining_owner = Owner::new(Some(Duration::from_secs(1)));
    let deadline = joining_owner.deadline.unwrap();
    let mut waiter = joining_owner.submit("observe completed failure", move || {
        let mut target = target;
        (target.join(), crate::checkpoint())
    });
    wait_until(deadline);
    assert!(!joining_owner.kernel.receive());
    joining_owner.drain();
    let returned = waiter.take_result();
    writeln!(
        std::io::stdout().lock(),
        "cross-join completed-panic target={target_id:?} returned={returned:?}"
    )
    .unwrap();
    let (joined, checkpoint) = returned.expect("typed result, not a disconnected observer channel");
    assert!(
        matches!(joined, Err(Error::TaskPanicked { task, .. }) if task == target_id),
        "{joined:?}"
    );
    assert!(matches!(checkpoint, Err(Error::DeadlineExceeded)));
    assert_eq!(joining_owner.kernel.stats.parks, 0);
    assert_eq!(joining_owner.kernel.shared.snapshot().active, 0);
    assert_eq!(target_owner.kernel.shared.snapshot().active, 0);
}
