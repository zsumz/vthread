use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use crate::{Error, ParkOutcome, Runtime, UnparkResult, support_test::run_isolated};

const UNSET: u8 = 0;
const REJECTED: u8 = 1;
const SUSPENDED: u8 = 2;
const UNEXPECTED: u8 = 3;

struct ParkOnDrop {
    parker: Arc<crate::parking::Parker>,
    outcome: Arc<AtomicU8>,
}

impl Drop for ParkOnDrop {
    fn drop(&mut self) {
        let outcome = match self.parker.park_timeout(Duration::from_secs(30)) {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(_) => SUSPENDED,
            Err(_) => UNEXPECTED,
        };
        self.outcome.store(outcome, Ordering::SeqCst);
    }
}

#[test]
fn rejected_panic_park_leaves_no_wait_timer_or_wake_state() {
    const CHILD: &str = "VTHREAD_PANIC_PARK_CHILD";
    const TEST: &str = "panic_parking_test::rejected_panic_park_leaves_no_wait_timer_or_wake_state";
    if std::env::var_os(CHILD).is_none() {
        let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(15));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !output.timed_out && stdout.contains("1 passed"),
            "panic-park child failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
            output.timed_out,
        );
        return;
    }

    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let (parker, unparker) = crate::parking::park_pair();
    let parker = Arc::new(parker);
    let drop_outcome = Arc::new(AtomicU8::new(UNSET));
    let sibling_panicking = Arc::new(AtomicBool::new(false));
    let wake_outcome = Arc::new(AtomicU8::new(UNSET));

    runtime
        .run_scope(|scope| {
            let (gate_entered, wait_for_gate) = mpsc::sync_channel(1);
            let (release_gate, gate_release) = mpsc::sync_channel(1);
            let mut gate = scope.spawn("admission gate", move || {
                gate_entered.send(()).unwrap();
                gate_release.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            wait_for_gate.recv_timeout(Duration::from_secs(5)).unwrap();

            let task_parker = Arc::clone(&parker);
            let task_outcome = Arc::clone(&drop_outcome);
            let mut unwinding = scope.spawn("panic park", move || {
                let _park_on_drop = ParkOnDrop {
                    parker: task_parker,
                    outcome: task_outcome,
                };
                panic!("expected task panic");
            })?;
            let observed = Arc::clone(&sibling_panicking);
            let observed_wake = Arc::clone(&wake_outcome);
            let mut sibling = scope.spawn("wake sibling", move || {
                observed.store(std::thread::panicking(), Ordering::SeqCst);
                let outcome = match unparker.unpark() {
                    UnparkResult::Stored => REJECTED,
                    UnparkResult::Woke | UnparkResult::Closed => SUSPENDED,
                };
                observed_wake.store(outcome, Ordering::SeqCst);
            })?;
            release_gate.send(()).unwrap();

            gate.join()?;
            assert!(matches!(unwinding.join(), Err(Error::TaskPanicked { .. })));
            sibling.join()?;
            let mut permit_consumer = scope.spawn("permit consumer", {
                let parker = Arc::clone(&parker);
                move || parker.park()
            })?;
            assert_eq!(permit_consumer.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .unwrap();

    assert_eq!(
        (
            drop_outcome.load(Ordering::SeqCst),
            wake_outcome.load(Ordering::SeqCst),
            sibling_panicking.load(Ordering::SeqCst),
        ),
        (REJECTED, REJECTED, false),
        "panic-time park published wait state or transferred panic state"
    );
    assert_drained(&runtime);
    runtime.shutdown().unwrap();
}

#[test]
fn rejected_panic_park_preserves_a_stored_permit() {
    const CHILD: &str = "VTHREAD_PANIC_STORED_PERMIT_CHILD";
    const TEST: &str = "panic_parking_test::rejected_panic_park_preserves_a_stored_permit";
    if std::env::var_os(CHILD).is_none() {
        let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(15));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !output.timed_out && stdout.contains("1 passed"),
            "stored-permit child failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
            output.timed_out,
        );
        return;
    }

    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let (parker, unparker) = crate::parking::park_pair();
    let parker = Arc::new(parker);
    let outcome = Arc::new(AtomicU8::new(UNSET));
    assert_eq!(unparker.unpark(), UnparkResult::Stored);

    runtime
        .run_scope(|scope| {
            let task_parker = Arc::clone(&parker);
            let task_outcome = Arc::clone(&outcome);
            let mut failed = scope.spawn("panic with stored permit", move || {
                let _park = ParkOnDrop {
                    parker: task_parker,
                    outcome: task_outcome,
                };
                panic!("expected stored-permit panic");
            })?;
            assert!(matches!(failed.join(), Err(Error::TaskPanicked { .. })));

            let mut consume = scope.spawn("consume preserved permit", {
                let parker = Arc::clone(&parker);
                move || parker.park()
            })?;
            assert_eq!(consume.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .unwrap();

    assert_eq!(outcome.load(Ordering::SeqCst), REJECTED);
    assert_drained(&runtime);
    runtime.shutdown().unwrap();
}

struct RegisteredParkOnDrop {
    parker: Arc<crate::parking::Parker>,
    callbacks: Arc<AtomicUsize>,
    outcome: Arc<AtomicU8>,
}

impl Drop for RegisteredParkOnDrop {
    fn drop(&mut self) {
        let callbacks = Arc::clone(&self.callbacks);
        let outcome = match self.parker.park_registered(move |_, _| {
            callbacks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }) {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(_) => SUSPENDED,
            Err(_) => UNEXPECTED,
        };
        self.outcome.store(outcome, Ordering::SeqCst);
    }
}

#[test]
fn panic_park_is_rejected_before_registration_or_generation_publication() {
    const CHILD: &str = "VTHREAD_PANIC_REGISTRATION_CHILD";
    const TEST: &str =
        "panic_parking_test::panic_park_is_rejected_before_registration_or_generation_publication";
    if std::env::var_os(CHILD).is_none() {
        let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(15));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !output.timed_out && stdout.contains("1 passed"),
            "panic-registration child failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
            output.timed_out,
        );
        return;
    }

    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let (parker, unparker) = crate::parking::park_pair();
    let parker = Arc::new(parker);
    let callbacks = Arc::new(AtomicUsize::new(0));
    let outcome = Arc::new(AtomicU8::new(UNSET));

    runtime
        .run_scope(|scope| {
            let task_parker = Arc::clone(&parker);
            let task_callbacks = Arc::clone(&callbacks);
            let task_outcome = Arc::clone(&outcome);
            let mut failed = scope.spawn("panic registration", move || {
                let _park = RegisteredParkOnDrop {
                    parker: task_parker,
                    callbacks: task_callbacks,
                    outcome: task_outcome,
                };
                panic!("expected registration panic");
            })?;
            assert!(matches!(failed.join(), Err(Error::TaskPanicked { .. })));

            let (registered, registration) = mpsc::sync_channel(1);
            let mut normal = scope.spawn("first real generation", {
                let parker = Arc::clone(&parker);
                move || {
                    parker.park_registered(move |token, _| {
                        registered.send(token.generation()).unwrap();
                        Ok(())
                    })
                }
            })?;
            assert_eq!(
                registration.recv_timeout(Duration::from_secs(5)).unwrap(),
                1
            );
            assert_eq!(unparker.unpark(), UnparkResult::Woke);
            assert_eq!(normal.join()??, ParkOutcome::Ready);
            Ok(())
        })
        .unwrap();

    assert_eq!(outcome.load(Ordering::SeqCst), REJECTED);
    assert_eq!(callbacks.load(Ordering::SeqCst), 0);
    assert_drained(&runtime);
    runtime.shutdown().unwrap();
}

fn assert_drained(runtime: &Runtime) {
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.parked, 0);
    assert_eq!(snapshot.timers, 0);
    assert!(
        snapshot
            .carriers
            .iter()
            .all(|carrier| carrier.pending_wakes == 0)
    );
}
