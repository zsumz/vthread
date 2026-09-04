use super::{Kernel, REMOTE_ADMISSION_YIELD_BOUND};
use crate::{CarrierId, Runtime, TaskFailure, control::Shared};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

#[test]
fn remote_starts_refill_a_bounded_runnable_window() {
    let config = Runtime::builder()
        .max_vthreads(65)
        .carrier_queue_capacity(65)
        .stack_cache_capacity(65)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    for _ in 0..65 {
        shared.submit(scope, "queued".into(), || ()).unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));

    assert!(kernel.receive());
    assert_eq!(kernel.ready.len(), 64);
    assert_eq!(kernel.inbox.pending(), 1);
    for _ in 0..32 {
        assert!(kernel.tick(true).unwrap());
        kernel.receive();
    }
    assert_eq!(kernel.ready.len(), 33);
    assert_eq!(kernel.inbox.pending(), 0);
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}

#[test]
fn yielding_window_cannot_starve_later_admissions() {
    let config = Runtime::builder()
        .max_vthreads(65)
        .carrier_queue_capacity(65)
        .stack_cache_capacity(65)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    for _ in 0..64 {
        shared
            .submit(scope, "yielding".into(), || {
                loop {
                    crate::yield_now().unwrap();
                }
            })
            .unwrap();
    }
    let later_ran = Arc::new(AtomicBool::new(false));
    let ran = Arc::clone(&later_ran);
    shared
        .submit(scope, "later".into(), move || {
            ran.store(true, Ordering::SeqCst);
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    for _ in 0..REMOTE_ADMISSION_YIELD_BOUND - 1 {
        assert!(kernel.tick(true).unwrap());
        kernel.receive();
    }
    assert_eq!(kernel.inbox.pending(), 1);
    assert!(!later_ran.load(Ordering::SeqCst));

    assert!(kernel.tick(true).unwrap());
    kernel.receive();
    assert_eq!(kernel.inbox.pending(), 0);
    for _ in 0..65 {
        assert!(kernel.tick(true).unwrap());
        kernel.receive();
    }
    assert!(later_ran.load(Ordering::SeqCst));
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}

#[test]
fn an_empty_remote_queue_does_not_precharge_admission_pressure() {
    let config = Runtime::builder()
        .max_vthreads(65)
        .carrier_queue_capacity(65)
        .stack_cache_capacity(65)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    for _ in 0..64 {
        shared
            .submit(scope, "yielding".into(), || {
                loop {
                    crate::yield_now().unwrap();
                }
            })
            .unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    assert!(!kernel.receive());

    for _ in 0..64 {
        assert!(kernel.tick(true).unwrap());
    }
    assert_eq!(kernel.yield_pressure, 0);

    shared.submit(scope, "later".into(), || ()).unwrap();
    assert!(kernel.receive());
    assert_eq!(kernel.inbox.pending(), 1);
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}

#[test]
fn pending_ingress_never_enters_the_signal_wait() {
    let config = Runtime::builder()
        .max_vthreads(1)
        .carrier_queue_capacity(1)
        .stack_cache_capacity(1)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.publish(crate::CarrierStatus::Running);
    shared.submit(scope, "queued".into(), || ()).unwrap();
    let observed = kernel.inbox.signal.version();
    let signal = Arc::clone(&kernel.inbox.signal);
    let (cancel, cancelled) = mpsc::channel();
    let rescue = std::thread::spawn(move || {
        if cancelled.recv_timeout(Duration::from_secs(1)).is_ok() {
            false
        } else {
            signal.notify();
            true
        }
    });

    kernel.wait_for_work(observed);
    let _ = cancel.send(());
    assert!(
        !rescue.join().unwrap(),
        "pending ingress entered signal wait"
    );
    assert_eq!(
        shared.snapshot().carriers[0].status,
        crate::CarrierStatus::Running
    );

    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}
