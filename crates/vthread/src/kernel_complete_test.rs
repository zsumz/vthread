use super::super::Kernel;
use crate::{CarrierId, Runtime, ScopeOptions, control::Shared};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn lifecycle_completion_lag_is_bounded_by_one_batch() {
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
        shared.submit(scope, "complete".into(), || ()).unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    for _ in 0..63 {
        assert!(kernel.tick(true).unwrap());
    }
    assert_eq!(shared.scope_report(scope).completed, 0);
    assert!(kernel.tick(true).unwrap());
    assert_eq!(shared.scope_report(scope).completed, 64);
    kernel.receive();
    assert!(kernel.tick(true).unwrap());
    assert_eq!(shared.scope_report(scope).completed, 65);
    shared.finish_scope(scope);
}

#[test]
fn changing_scope_flushes_completion_before_dispatch() {
    let config = Runtime::builder()
        .max_vthreads(2)
        .max_owned_scopes(2)
        .carrier_queue_capacity(2)
        .stack_cache_capacity(2)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let first = shared.begin_scope().unwrap();
    let second = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    shared.submit(first, "first".into(), || ()).unwrap();
    let observer = Arc::clone(&shared);
    shared
        .submit(second, "observer".into(), move || {
            assert_eq!(observer.scope_report(first).completed, 1);
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    assert!(kernel.tick(true).unwrap());
    assert_eq!(shared.scope_report(first).completed, 0);
    assert!(kernel.tick(true).unwrap());
    shared.finish_scope(first);
    shared.finish_scope(second);
}

#[test]
fn target_waiter_forces_prompt_completion_publication() {
    let config = Runtime::builder()
        .max_vthreads(2)
        .carrier_queue_capacity(2)
        .stack_cache_capacity(2)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let target = shared.submit(scope, "target".into(), || ()).unwrap();
    let waiting_for = Arc::clone(&target.record);
    shared.submit(scope, "sibling".into(), || ()).unwrap();
    let observer = Arc::clone(&shared);
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || {
        observer.wait(scope, Some(&waiting_for)).unwrap();
        sent.send(()).unwrap();
    });
    crate::support_test::until(|| !shared.may_defer_completion());
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();

    assert!(kernel.tick(true).unwrap());
    received
        .recv_timeout(Duration::from_secs(1))
        .expect("target completion remained batched");
    assert_eq!(shared.scope_report(scope).completed, 1);
    assert!(kernel.tick(true).unwrap());
    waiter.join().unwrap();
    shared.finish_scope(scope);
}
