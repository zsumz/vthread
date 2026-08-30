use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{Error, ParkOutcome, Runtime, UnparkResult, park_pair, yield_now};

#[test]
fn parking_outside_a_virtual_thread_is_rejected() {
    let (parker, _unparker) = park_pair();
    assert!(matches!(parker.park(), Err(Error::OutsideVThread)));
}

#[test]
fn a_ready_task_wakes_one_parked_generation() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Rc::new(RefCell::new(Vec::new()));
    let (parker, unparker) = park_pair();

    runtime
        .scope(|scope| {
            let waiter_trace = Rc::clone(&trace);
            let waiter = scope.spawn("waiter", move || {
                waiter_trace.borrow_mut().push("park");
                let outcome = parker.park().expect("park task");
                waiter_trace.borrow_mut().push("resume");
                outcome
            })?;
            let waker_trace = Rc::clone(&trace);
            let waker = scope.spawn("waker", move || {
                waker_trace.borrow_mut().push("wake");
                unparker.unpark()
            })?;

            assert_eq!(waker.join()?, UnparkResult::Woke);
            assert_eq!(waiter.join()?, ParkOutcome::Ready);
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(&*trace.borrow(), &["park", "wake", "resume"]);
}

#[test]
fn a_preexisting_unpark_is_consumed_without_suspension() {
    let runtime = Runtime::new().expect("build runtime");
    let (parker, unparker) = park_pair();
    assert_eq!(unparker.unpark(), UnparkResult::Stored);

    runtime
        .scope(|scope| {
            let waiter = scope.spawn("waiter", move || parker.park())?;
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
        .scope(|scope| {
            let waiter = scope.spawn("waiter", move || parker.park_timeout(Duration::ZERO))?;
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
    let parker = Rc::new(parker);

    runtime
        .scope(|scope| {
            let first_parker = Rc::clone(&parker);
            let first = scope.spawn("first", move || first_parker.park())?;
            let second_parker = Rc::clone(&parker);
            let second = scope.spawn("second", move || second_parker.park())?;

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
        .scope(|scope| {
            let waiter = scope.spawn("waiter", move || {
                let first = parker.park().expect("first park");
                let second = parker
                    .park_timeout(Duration::from_millis(1))
                    .expect("second park");
                (first, second)
            })?;
            scope.spawn("cancel", move || {
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
        .scope(|scope| {
            let waiter = scope.spawn("waiter", move || {
                let first = parker.park().expect("first park");
                let second = parker.park().expect("closed park");
                (first, second)
            })?;
            scope.spawn("close", move || {
                assert!(closer.close());
            })?;
            assert_eq!(waiter.join()?, (ParkOutcome::Closed, ParkOutcome::Closed));
            Ok(())
        })
        .expect("scope succeeds");
    assert!(unparker.is_closed());
}
