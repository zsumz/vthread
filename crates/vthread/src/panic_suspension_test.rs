use std::{
    panic,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use crate::{Error, Runtime, support_test::run_isolated};

const UNSET: u8 = 0;
const REJECTED: u8 = 1;
const SUSPENDED: u8 = 2;
const UNEXPECTED: u8 = 3;

struct YieldOnDrop(Arc<AtomicU8>);

impl Drop for YieldOnDrop {
    fn drop(&mut self) {
        let outcome = match crate::yield_now() {
            Err(Error::SuspensionDuringPanic) => REJECTED,
            Ok(()) => SUSPENDED,
            Err(_) => UNEXPECTED,
        };
        self.0.store(outcome, Ordering::SeqCst);
    }
}

#[test]
fn unwinding_task_cannot_expose_carrier_panic_state_to_sibling() {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let drop_outcome = Arc::new(AtomicU8::new(UNSET));
    let sibling_panicking = Arc::new(AtomicBool::new(false));

    runtime
        .run_scope(|scope| {
            let (gate_entered, wait_for_gate) = mpsc::sync_channel(1);
            let (release_gate, gate_release) = mpsc::sync_channel(1);
            let mut gate = scope.spawn("admission gate", move || {
                gate_entered.send(()).unwrap();
                gate_release.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            wait_for_gate.recv_timeout(Duration::from_secs(5)).unwrap();

            let task_outcome = Arc::clone(&drop_outcome);
            let mut unwinding = scope.spawn("unwinding", move || {
                let _yield_on_drop = YieldOnDrop(task_outcome);
                panic!("expected task panic");
            })?;
            let observed = Arc::clone(&sibling_panicking);
            let mut sibling = scope.spawn("sibling", move || {
                observed.store(std::thread::panicking(), Ordering::SeqCst);
            })?;
            release_gate.send(()).unwrap();

            gate.join()?;
            assert!(matches!(unwinding.join(), Err(Error::TaskPanicked { .. })));
            sibling.join()?;
            Ok(())
        })
        .unwrap();

    assert_eq!(
        (
            drop_outcome.load(Ordering::SeqCst),
            sibling_panicking.load(Ordering::SeqCst),
        ),
        (REJECTED, false),
        "an unwinding task transferred carrier panic state to its sibling"
    );
    runtime.shutdown().unwrap();
}

#[test]
fn panic_hook_cannot_transfer_control_to_another_task() {
    const CHILD: &str = "VTHREAD_PANIC_HOOK_SUSPENSION_CHILD";
    const TEST: &str = "panic_suspension_test::panic_hook_cannot_transfer_control_to_another_task";
    if std::env::var_os(CHILD).is_none() {
        let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(15));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && !output.timed_out && stdout.contains("1 passed"),
            "panic-hook suspension child failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
            output.timed_out,
        );
        return;
    }

    let hook_calls = Arc::new(AtomicUsize::new(0));
    let hook_outcome = Arc::new(AtomicU8::new(UNSET));
    let observed_calls = Arc::clone(&hook_calls);
    let observed_outcome = Arc::clone(&hook_outcome);
    panic::set_hook(Box::new(move |_| {
        if observed_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            let outcome = match crate::yield_now() {
                Err(Error::SuspensionDuringPanic) => REJECTED,
                Ok(()) => SUSPENDED,
                Err(_) => UNEXPECTED,
            };
            observed_outcome.store(outcome, Ordering::SeqCst);
        }
    }));

    let runtime = Runtime::builder().carriers(1).build().unwrap();
    runtime
        .run_scope(|scope| {
            let (gate_entered, wait_for_gate) = mpsc::sync_channel(1);
            let (release_gate, gate_release) = mpsc::sync_channel(1);
            let mut gate = scope.spawn("admission gate", move || {
                gate_entered.send(()).unwrap();
                gate_release.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            wait_for_gate.recv_timeout(Duration::from_secs(5)).unwrap();

            let mut first = scope.spawn("first panic", move || {
                panic!("first expected panic");
            })?;
            let mut second = scope.spawn("second panic", || panic!("second expected panic"))?;
            release_gate.send(()).unwrap();
            gate.join()?;
            assert!(matches!(first.join(), Err(Error::TaskPanicked { .. })));
            assert!(matches!(second.join(), Err(Error::TaskPanicked { .. })));
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
    drop(panic::take_hook());

    assert_eq!(hook_calls.load(Ordering::SeqCst), 2);
    assert_eq!(hook_outcome.load(Ordering::SeqCst), REJECTED);
}
