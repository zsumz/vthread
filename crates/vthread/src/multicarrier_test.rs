use std::{
    rc::Rc,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use crate::support_test::until;
use crate::{CarrierId, ParkOutcome, Runtime, TaskStatus, UnparkResult, park_pair, yield_now};

#[test]
fn carriers_run_simultaneously_and_stacks_keep_their_owner_across_remote_wakes() {
    let runtime = Runtime::builder().carriers(2).build().expect("runtime");
    let (started_tx, started_rx) = mpsc::sync_channel(2);
    let owners = runtime
        .scope(|scope| {
            let mut jobs = Vec::new();
            let mut releases = Vec::new();
            let mut wakers = Vec::new();
            for index in 0..2 {
                let started = started_tx.clone();
                let (release_tx, release_rx) = mpsc::sync_channel(1);
                let (parker, unparker) = park_pair();
                releases.push(release_tx);
                wakers.push(unparker);
                jobs.push(scope.spawn("affine", move || {
                    let owner = thread::current().id();
                    // Non-Send state is created only after placement, then survives suspension.
                    let value = Rc::new(index);
                    started.send(owner).expect("started");
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("concurrent carriers");
                    for _ in 0..64 {
                        yield_now().expect("yield");
                        assert_eq!(thread::current().id(), owner);
                    }
                    assert_eq!(
                        parker.park_timeout(Duration::from_secs(5)).expect("park"),
                        ParkOutcome::Ready
                    );
                    assert_eq!(thread::current().id(), owner);
                    assert_eq!(*value, index);
                    owner
                })?);
            }
            let first = started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("first carrier");
            let second = started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("second carrier");
            assert_ne!(
                first, second,
                "both carriers must run while the other is blocked"
            );
            let placement = scope
                .snapshot()
                .tasks
                .iter()
                .map(|task| task.carrier)
                .collect::<Vec<_>>();
            assert_eq!(placement, vec![CarrierId(0), CarrierId(1)]);
            for release in releases {
                release.send(()).expect("release");
            }
            until(|| scope.snapshot().parked == 2);
            for waker in wakers {
                assert_eq!(
                    thread::spawn(move || waker.unpark())
                        .join()
                        .expect("remote unpark"),
                    UnparkResult::Woke
                );
            }
            let mut owners = Vec::new();
            for job in jobs {
                owners.push(job.join()?);
            }
            let snapshot = scope.snapshot();
            assert_eq!(
                snapshot
                    .tasks
                    .iter()
                    .map(|task| task.carrier)
                    .collect::<Vec<_>>(),
                placement
            );
            assert!(
                snapshot
                    .tasks
                    .iter()
                    .all(|task| task.mounts == 66 && task.status == TaskStatus::Completed)
            );
            Ok(owners)
        })
        .expect("scope");

    // A later scope runs on the same persistent OS threads.
    runtime
        .scope(|scope| {
            for _ in 0..8 {
                let owner = scope
                    .spawn("reuse-carrier", || thread::current().id())?
                    .join()?;
                assert!(owners.contains(&owner));
            }
            Ok(())
        })
        .expect("persistent carriers");
}

#[test]
fn external_unpark_interrupts_the_carriers_long_timer_wait() {
    let runtime = Runtime::new().expect("runtime");
    let (parker, unparker) = park_pair();
    runtime
        .scope(|scope| {
            let waiter = scope.spawn("remote", move || {
                let before = thread::current().id();
                let outcome = parker.park_timeout(Duration::from_secs(5)).expect("park");
                (before, thread::current().id(), outcome)
            })?;
            until(|| scope.snapshot().parked == 1);
            assert_eq!(
                thread::spawn(move || unparker.unpark())
                    .join()
                    .expect("waker"),
                UnparkResult::Woke
            );
            let (before, after, outcome) = waiter.join()?;
            assert_eq!(before, after);
            assert_eq!(outcome, ParkOutcome::Ready);
            assert_eq!(scope.snapshot().stats.timeouts, 0);
            Ok(())
        })
        .expect("scope");
}

#[test]
fn stall_recovery_can_be_disabled_for_external_waits() {
    let runtime = Runtime::builder()
        .stall_policy(crate::StallPolicy::Disabled)
        .build()
        .expect("runtime");
    let (parker, unparker) = park_pair();
    runtime
        .scope(|scope| {
            let task = scope.spawn("external", move || parker.park())?;
            until(|| scope.snapshot().parked == 1);
            assert_eq!(unparker.unpark(), UnparkResult::Woke);
            assert_eq!(task.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .expect("scope");
}

#[test]
fn runtime_and_remote_wakers_are_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Runtime>();
    send_sync::<crate::Unparker>();
    send_sync::<Arc<Runtime>>();
}

#[test]
fn remote_unpark_racing_registration_never_loses_a_permit() {
    let runtime = Runtime::new().expect("runtime");
    let (parker, unparker) = park_pair();
    let (request_tx, request_rx) = mpsc::sync_channel(1);
    let remote = thread::spawn(move || {
        for _ in 0..256 {
            request_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("wake request");
            unparker.unpark();
        }
    });
    runtime
        .scope(|scope| {
            scope
                .spawn("registration-race", move || {
                    for _ in 0..256 {
                        request_tx.send(()).expect("request");
                        assert_eq!(
                            parker.park_timeout(Duration::from_secs(1)).expect("park"),
                            ParkOutcome::Ready
                        );
                    }
                })?
                .join()
        })
        .expect("all wake permits observed");
    remote.join().expect("remote waker");
}

#[test]
fn repeated_generation_progress_resets_the_quiescent_scope_grace() {
    let runtime = Runtime::builder()
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(500)))
        .build()
        .expect("runtime");
    let (parker, unparker) = park_pair();
    let (request_tx, request_rx) = mpsc::sync_channel(1);
    let remote = thread::spawn(move || {
        for _ in 0..64 {
            request_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("wake request");
            thread::park_timeout(Duration::from_millis(10));
            unparker.unpark();
        }
    });
    runtime
        .scope(|scope| {
            scope
                .spawn("progress", move || {
                    for _ in 0..64 {
                        request_tx.send(()).expect("request");
                        assert_eq!(parker.park().expect("park"), ParkOutcome::Ready);
                    }
                })?
                .join()
        })
        .expect("progress lasts longer than one grace period without stalling");
    remote.join().expect("waker");
}
