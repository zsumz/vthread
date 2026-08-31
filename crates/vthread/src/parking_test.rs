use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::support_test::until;
use crate::{Error, ParkOutcome, Runtime, UnparkResult, park_pair, yield_now};

#[test]
fn parking_outside_a_virtual_thread_is_rejected() {
    let (parker, _unparker) = park_pair();
    assert!(matches!(parker.park(), Err(Error::OutsideVThread)));
}

#[test]
fn a_ready_task_wakes_one_parked_generation() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let (parker, unparker) = park_pair();

    runtime
        .run_scope(|scope| {
            let waiter_trace = Arc::clone(&trace);
            let mut waiter = scope.spawn("waiter", move || {
                waiter_trace.lock().expect("trace").push("park");
                let outcome = parker.park().expect("park task");
                waiter_trace.lock().expect("trace").push("resume");
                outcome
            })?;
            until(|| scope.runtime_snapshot().parked == 1);
            let waker_trace = Arc::clone(&trace);
            let mut waker = scope.spawn("waker", move || {
                waker_trace.lock().expect("trace").push("wake");
                unparker.unpark()
            })?;

            assert_eq!(waker.join()?, UnparkResult::Woke);
            assert_eq!(waiter.join()?, ParkOutcome::Ready);
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(&*trace.lock().expect("trace"), &["park", "wake", "resume"]);
}

#[test]
fn a_preexisting_unpark_is_consumed_without_suspension() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, unparker) = park_pair();
    assert_eq!(unparker.unpark(), UnparkResult::Stored);

    runtime
        .run_scope(|scope| {
            let mut waiter = scope.spawn("waiter", move || parker.park())?;
            assert_eq!(waiter.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .expect("scope succeeds");
    assert_eq!(runtime.snapshot().stats.parks, 0);
}

#[test]
fn an_expired_timeout_does_not_enter_the_parked_set() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, _unparker) = park_pair();

    runtime
        .run_scope(|scope| {
            let mut waiter = scope.spawn("waiter", move || parker.park_timeout(Duration::ZERO))?;
            assert_eq!(waiter.join()??, ParkOutcome::TimedOut);
            Ok(())
        })
        .expect("scope succeeds");
    assert_eq!(runtime.snapshot().stats.parks, 0);
}

#[test]
fn one_parker_rejects_two_active_consumers() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, unparker) = park_pair();
    let parker = Arc::new(parker);

    runtime
        .run_scope(|scope| {
            let first_parker = Arc::clone(&parker);
            let mut first = scope.spawn("first", move || first_parker.park())?;
            until(|| scope.runtime_snapshot().parked == 1);
            let second_parker = Arc::clone(&parker);
            let mut second = scope.spawn("second", move || second_parker.park())?;

            assert!(matches!(second.join()?, Err(Error::ParkerBusy)));
            assert_eq!(unparker.unpark(), UnparkResult::Woke);
            assert_eq!(first.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .expect("scope succeeds");
}

#[test]
fn cancellation_wins_without_closing_future_generations() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, unparker) = park_pair();
    let canceller = unparker.clone();

    runtime
        .run_scope(|scope| {
            let mut waiter = scope.spawn("waiter", move || {
                let first = parker.park().expect("first park");
                let second = parker
                    .park_timeout(Duration::from_millis(1))
                    .expect("second park");
                (first, second)
            })?;
            until(|| scope.runtime_snapshot().parked == 1);
            let _ = scope.spawn("cancel", move || {
                yield_now().expect("mounted task");
                assert!(canceller.cancel());
            })?;
            let (first, second) = waiter.join()?;
            assert_eq!(first, ParkOutcome::Cancelled);
            assert_eq!(second, ParkOutcome::TimedOut);
            Ok(())
        })
        .expect("scope succeeds");
}

#[test]
fn close_wakes_and_is_terminal() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, unparker) = park_pair();
    let closer = unparker.clone();

    runtime
        .run_scope(|scope| {
            let mut waiter = scope.spawn("waiter", move || {
                let first = parker.park().expect("first park");
                let second = parker.park().expect("closed park");
                (first, second)
            })?;
            until(|| scope.runtime_snapshot().parked == 1);
            let _ = scope.spawn("close", move || {
                assert!(closer.close());
            })?;
            assert_eq!(waiter.join()?, (ParkOutcome::Closed, ParkOutcome::Closed));
            Ok(())
        })
        .expect("scope succeeds");
    assert!(unparker.is_closed());
}

#[test]
fn selected_winners_are_not_replaced_by_inherited_cancellation() {
    for selected in [
        ParkOutcome::Ready,
        ParkOutcome::Closed,
        ParkOutcome::Cancelled,
        ParkOutcome::TimedOut,
    ] {
        let runtime = Runtime::new().unwrap();
        runtime
            .run_scope(|scope| {
                let cancellation = scope.cancellation_token();
                let mut task = scope.spawn("selected-winner", move || {
                    let (parker, unparker) = park_pair();
                    let result = parker.park_registered(|token, wake| {
                        match selected {
                            ParkOutcome::Ready => {
                                assert!(wake.select_ready(token));
                            }
                            ParkOutcome::Closed => {
                                assert!(wake.select_closed(token));
                            }
                            ParkOutcome::Cancelled => {
                                assert!(unparker.cancel());
                            }
                            ParkOutcome::TimedOut => {
                                assert!(wake.select_timeout(token)?);
                            }
                        }
                        cancellation.cancel();
                        Ok(())
                    });
                    (result, crate::checkpoint())
                })?;
                let (outcome, checkpoint) = task.join()?;
                assert!(
                    matches!(outcome, Ok(winner) if winner == selected),
                    "{outcome:?}"
                );
                assert!(matches!(checkpoint, Err(Error::Cancelled)));
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn inherited_cancellation_is_an_error_when_it_selects_first() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let cancellation = scope.cancellation_token();
            let mut task = scope.spawn("inherited-winner", move || {
                let (parker, _) = park_pair();
                parker.park_registered(|token, wake| {
                    cancellation.cancel();
                    assert!(!wake.select_ready(token));
                    Ok(())
                })
            })?;
            assert!(matches!(task.join()?, Err(Error::Cancelled)));
            Ok(())
        })
        .unwrap();
}
