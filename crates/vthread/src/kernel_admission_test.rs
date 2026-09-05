use super::{Kernel, REMOTE_ADMISSION_DISPATCH_BOUND, REMOTE_READY_TARGET};
use crate::{CarrierId, Runtime, TaskFailure, control::Shared};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn mixed_parks_yields_and_wakes_cannot_starve_a_late_start() {
    check_mixed_progress(false);
}

#[test]
fn borrowed_ready_work_cannot_starve_a_late_remote_start() {
    check_mixed_progress(true);
}

fn check_mixed_progress(borrowed: bool) {
    let capacity = REMOTE_READY_TARGET + 2;
    let shared = shared(capacity);
    let scope = shared.begin_scope().unwrap();
    let (parker, waker) = crate::park_pair();
    let stopped = Arc::new(AtomicBool::new(false));
    if borrowed {
        let stop = Arc::clone(&stopped);
        shared
            .submit(scope, "parent".into(), move || {
                crate::local_scope(|local| {
                    let _ = local.spawn("park", || park_until_stopped(&parker, &stop))?;
                    for _ in 1..REMOTE_READY_TARGET {
                        let _ = local.spawn("yield", || yield_until_stopped(&stop))?;
                    }
                    Ok(())
                })
                .unwrap();
            })
            .unwrap();
    } else {
        let stop = Arc::clone(&stopped);
        shared
            .submit(scope, "park".into(), move || {
                park_until_stopped(&parker, &stop);
            })
            .unwrap();
        for _ in 1..REMOTE_READY_TARGET {
            let stop = Arc::clone(&stopped);
            shared
                .submit(scope, "yield".into(), move || yield_until_stopped(&stop))
                .unwrap();
        }
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    assert!(!kernel.receive());
    if borrowed {
        assert!(kernel.tick(true).unwrap());
        kernel.receive_local();
        assert!(kernel.has_borrowed);
    }
    assert_eq!(kernel.ready.len(), REMOTE_READY_TARGET);

    let late = Arc::new(AtomicBool::new(false));
    let ran = Arc::clone(&late);
    let owner = std::thread::current().id();
    shared
        .submit(scope, "late".into(), move || {
            assert_eq!(std::thread::current().id(), owner);
            ran.store(true, Ordering::Relaxed);
        })
        .unwrap();
    assert!(kernel.receive());
    for _ in 0..REMOTE_ADMISSION_DISPATCH_BOUND {
        assert!(kernel.tick(false).unwrap());
        if kernel.stats.parks > kernel.stats.wakes + u64::from(borrowed) {
            assert_eq!(waker.unpark(), crate::UnparkResult::Woke);
        }
        kernel.receive();
        assert!(kernel.ready.len() > REMOTE_READY_TARGET / 2);
        assert!(kernel.tasks.len() <= capacity);
    }
    let pending_at_bound = kernel.inbox.pending();
    for _ in 0..4 * (REMOTE_READY_TARGET + 1) {
        assert!(kernel.tick(false).unwrap());
        if kernel.stats.parks > kernel.stats.wakes + u64::from(borrowed) {
            waker.unpark();
        }
        kernel.receive();
    }
    let ran_within_bound = late.load(Ordering::Relaxed);
    let (parks, yields) = (kernel.stats.parks, kernel.stats.yields);
    // Complete real borrowed scopes before asserting the old policy's failure.
    stopped.store(true, Ordering::Relaxed);
    waker.unpark();
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
    assert!(parks > 1_000 && yields > 1_000, "missing mixed traffic");
    assert_eq!(pending_at_bound, 0, "parking erased admission progress");
    assert!(
        ran_within_bound,
        "admitted task missed its ready-queue bound"
    );
}

#[test]
fn quota_service_materializes_one_start_not_the_whole_backlog() {
    let shared = shared(REMOTE_READY_TARGET * 3);
    let scope = shared.begin_scope().unwrap();
    for _ in 0..REMOTE_READY_TARGET * 3 {
        shared.submit(scope, "queued".into(), || ()).unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert_eq!(kernel.ready.len(), REMOTE_READY_TARGET);
    let pending = kernel.inbox.pending();
    kernel.admission_pressure = REMOTE_ADMISSION_DISPATCH_BOUND;
    kernel.receive();
    let (ready, remaining) = (kernel.ready.len(), kernel.inbox.pending());
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
    assert_eq!(ready, REMOTE_READY_TARGET + 1, "unbounded quota batch");
    assert_eq!(remaining, pending - 1);
}

#[test]
fn completions_preserve_credit_until_pending_admission_makes_progress() {
    let shared = shared(REMOTE_READY_TARGET + 1);
    let scope = shared.begin_scope().unwrap();
    for _ in 0..=REMOTE_READY_TARGET {
        shared.submit(scope, "complete".into(), || ()).unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    assert!(kernel.receive());
    kernel.admission_pressure = REMOTE_ADMISSION_DISPATCH_BOUND - 2;
    assert!(kernel.tick(false).unwrap());
    assert!(kernel.receive());
    assert_eq!(
        kernel.admission_pressure,
        REMOTE_ADMISSION_DISPATCH_BOUND - 1
    );
    assert_eq!(kernel.inbox.pending(), 1);
    assert!(kernel.tick(false).unwrap());
    assert!(!kernel.receive());
    assert_eq!(kernel.admission_pressure, 0);
    assert_eq!(kernel.ready.len(), REMOTE_READY_TARGET - 1);
    kernel.abort(None, TaskFailure::RuntimeStopped);
    shared.finish_scope(scope);
}

fn shared(capacity: usize) -> Arc<Shared> {
    let config = Runtime::builder()
        .max_vthreads(capacity)
        .carrier_queue_capacity(capacity)
        .stack_cache_capacity(capacity)
        .build()
        .unwrap()
        .config();
    Arc::new(Shared::new(config))
}

fn yield_until_stopped(stop: &AtomicBool) {
    let owner = std::thread::current().id();
    while !stop.load(Ordering::Relaxed) {
        crate::yield_now().unwrap();
        assert_eq!(std::thread::current().id(), owner);
    }
}

fn park_until_stopped(parker: &crate::Parker, stop: &AtomicBool) {
    let owner = std::thread::current().id();
    while !stop.load(Ordering::Relaxed) {
        parker.park().unwrap();
        assert_eq!(std::thread::current().id(), owner);
    }
}
