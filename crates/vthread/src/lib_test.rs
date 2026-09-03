use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use crate::{Error, Runtime, run, yield_now};

#[test]
fn convenience_run_executes_a_scope() {
    let value = run(|scope| scope.spawn("answer", || 42)?.join()).expect("run succeeds");
    assert_eq!(value, 42);
}

#[test]
fn yielding_outside_a_virtual_thread_is_an_error() {
    assert!(matches!(yield_now(), Err(Error::OutsideVThread)));
}

#[test]
fn yielding_checks_cancellation_after_resumption() {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    runtime
        .run_scope(|scope| {
            let (gate_entered, wait_for_gate) = mpsc::sync_channel(1);
            let (release_gate, gate_release) = mpsc::sync_channel(1);
            let mut gate = scope.spawn("hold carrier", move || {
                gate_entered.send(()).unwrap();
                gate_release.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            wait_for_gate.recv_timeout(Duration::from_secs(5)).unwrap();

            let continued = Arc::new(AtomicBool::new(false));
            let continued_after_yield = Arc::clone(&continued);
            let mut yielding = scope.spawn("cancel while yielded", move || {
                yield_now()?;
                continued_after_yield.store(true, Ordering::Release);
                Ok(())
            })?;

            let (barrier_entered, wait_for_barrier) = mpsc::sync_channel(1);
            let (release_barrier, barrier_release) = mpsc::sync_channel(1);
            let mut barrier = scope.spawn("hold yielded task", move || {
                barrier_entered.send(()).unwrap();
                barrier_release
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            })?;

            release_gate.send(()).unwrap();
            wait_for_barrier
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            yielding.cancel();
            release_barrier.send(()).unwrap();

            gate.join()?;
            barrier.join()?;
            assert!(matches!(yielding.join()?, Err(Error::Cancelled)));
            assert!(!continued.load(Ordering::Acquire));
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn yielding_checks_cancellation_before_giving_up_the_carrier() {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    runtime
        .run_scope(|scope| {
            let (gate_entered, wait_for_gate) = mpsc::sync_channel(1);
            let (release_gate, gate_release) = mpsc::sync_channel(1);
            let mut gate = scope.spawn("hold carrier", move || {
                gate_entered.send(()).unwrap();
                gate_release.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            wait_for_gate.recv_timeout(Duration::from_secs(5)).unwrap();

            let sibling_ran = Arc::new(AtomicBool::new(false));
            let observed = Arc::clone(&sibling_ran);
            let mut cancelled = scope.spawn("cancel before yield", move || {
                crate::cancellation_token()?.cancel();
                assert!(matches!(yield_now(), Err(Error::Cancelled)));
                Ok::<_, Error>(observed.load(Ordering::Acquire))
            })?;
            let sibling_ran_task = Arc::clone(&sibling_ran);
            let mut sibling = scope.spawn("queued sibling", move || {
                sibling_ran_task.store(true, Ordering::Release);
            })?;

            release_gate.send(()).unwrap();
            gate.join()?;
            assert!(!cancelled.join()??);
            sibling.join()?;
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
}
