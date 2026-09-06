use super::{Grant, MutexCall, grant};
use crate::{
    CarrierId, JoinHandle, Runtime, RuntimeConfig, control::Shared, kernel::Kernel,
    local_carrier::LocalCarrier, sync::Mutex, wait::WaitHub,
};
use std::{rc::Rc, sync::Arc};

#[test]
fn calls_partition_success_error_and_unwind_without_counting_native_callers() {
    let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    {
        let _route = crate::context::mount_carrier(&hub, &local);
        let mut call = MutexCall::new();
        call.acquired();
        drop(call);
        drop(MutexCall::new());
        ::core::assert!(
            std::panic::catch_unwind(|| {
                let _call = MutexCall::new();
                ::core::panic!("incomplete mutex call");
            })
            .is_err()
        );
        for outcome in [
            Grant::Stored,
            Grant::SameOwner,
            Grant::OtherOwner,
            Grant::Rejected,
        ] {
            grant(outcome);
        }
    }
    let before = *local.handoff_profile.borrow();
    drop(MutexCall::new());
    grant(Grant::Stored);
    let after = *local.handoff_profile.borrow();
    ::core::assert_eq!(before, after, "unmounted caller changed owner-local counts");
    let counts = after.mutex();
    ::core::assert_eq!(counts.calls(), 3);
    ::core::assert_eq!(counts.acquired(), 1);
    ::core::assert_eq!(counts.failed_calls(), 2);
    ::core::assert_eq!(counts.attempts(), 4);
    ::core::assert_eq!(counts.stored(), 1);
    ::core::assert_eq!(counts.same_owner(), 1);
    ::core::assert_eq!(counts.other_owner(), 1);
    ::core::assert_eq!(counts.rejected(), 1);
}

#[test]
fn immediate_mutex_calls_do_not_attach_tickets_or_count_try_lock() {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("immediate mutex", || {
                    let mutex = Mutex::new(0);
                    for _ in 0..4 {
                        *mutex.lock().unwrap() += 1;
                    }
                    ::core::assert_eq!(*mutex.try_lock().unwrap(), 4);
                })?
                .join()
        })
        .unwrap();
    runtime.shutdown().unwrap();
    let snapshot = runtime.snapshot();
    let counts = snapshot.carriers()[0].handoff_profile().mutex();
    ::core::assert_eq!(counts.calls(), 4);
    ::core::assert_eq!(counts.acquired(), 4);
    ::core::assert_eq!(counts.immediate(), 4);
    ::core::assert_eq!(counts.recheck(), 0);
    ::core::assert_eq!(counts.queued(), 0);
    ::core::assert_eq!(counts.parks(), 0);
    ::core::assert_eq!(counts.attempts(), 0);
}

#[test]
fn ordered_cancellation_distinguishes_queued_removal_and_selected_abandonment() {
    for selected in [false, true] {
        let config = Runtime::builder()
            .carriers(1)
            .max_vthreads(2)
            .stack_cache_capacity(2)
            .carrier_queue_capacity(2)
            .build()
            .unwrap()
            .config();
        let shared = Arc::new(Shared::new(config));
        let scope = shared.begin_scope().unwrap();
        let mutex = Arc::new(Mutex::new(42));
        let owner = mutex.try_lock().unwrap();
        let waiting = Arc::clone(&mutex);
        let spawned = shared
            .submit(scope, "cancelled mutex".into(), move || {
                waiting.lock().map(drop)
            })
            .unwrap();
        let mut child = JoinHandle::new(
            Arc::clone(&shared),
            spawned.id,
            spawned.cell,
            spawned.record,
        );
        let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
        let _route = crate::context::mount_carrier(&kernel.inbox.hub, &kernel.local);
        kernel.receive();
        ::core::assert!(kernel.tick(false).unwrap());
        ::core::assert_eq!(mutex.waiting(), 1);
        ::core::assert_eq!(kernel.stats.parks, 1);
        if selected {
            drop(owner);
            child.cancel();
            while kernel.tick(true).unwrap() {}
        } else {
            child.cancel();
            while kernel.tick(true).unwrap() {}
            ::core::assert!(
                child.is_finished(),
                "cancellation must precede resource release"
            );
            drop(owner);
        }
        ::core::assert!(::core::matches!(
            child.take_result().unwrap(),
            Err(crate::Error::Cancelled)
        ));
        ::core::assert_eq!(mutex.waiting(), 0);
        ::core::assert_eq!(*mutex.try_lock().unwrap(), 42);
        let profile = kernel.local.handoff_profile.borrow();
        let counts = profile.mutex();
        ::core::assert_eq!(counts.calls(), 1);
        ::core::assert_eq!(counts.failed_calls(), 1);
        ::core::assert_eq!(counts.acquired(), 0);
        ::core::assert_eq!(counts.queued(), 1);
        ::core::assert_eq!(counts.dropped(), 1);
        ::core::assert_eq!(counts.removed(), u64::from(!selected));
        ::core::assert_eq!(counts.abandoned(), u64::from(selected));
        ::core::assert_eq!(counts.completed(), 0);
        ::core::assert_eq!(counts.parks(), 1);
        ::core::assert_eq!(
            counts.park_returns(),
            0,
            "resume checkpoint rejected acquisition"
        );
        ::core::assert_eq!(counts.same_owner(), u64::from(selected));
        ::core::assert_eq!(counts.attempts(), u64::from(selected));
        ::core::assert_eq!(counts.other_owner(), 0);
        ::core::assert_eq!(counts.stored(), 0);
        drop(profile);
        shared.finish_scope(scope);
    }
}

#[test]
fn useful_handoff_receives_ownership_once_and_retires_its_ticket() {
    let config = Runtime::builder()
        .carriers(1)
        .max_vthreads(2)
        .stack_cache_capacity(2)
        .carrier_queue_capacity(2)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let mutex = Arc::new(Mutex::new(42));
    let owner = mutex.try_lock().unwrap();
    let waiting = Arc::clone(&mutex);
    shared
        .submit(scope, "useful mutex".into(), move || {
            *waiting.lock().unwrap() += 1;
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    let _route = crate::context::mount_carrier(&kernel.inbox.hub, &kernel.local);
    kernel.receive();
    ::core::assert!(kernel.tick(false).unwrap());
    ::core::assert_eq!(mutex.waiting(), 1);
    drop(owner);
    while kernel.tick(false).unwrap() {}
    ::core::assert_eq!(*mutex.try_lock().unwrap(), 43);
    ::core::assert_eq!(mutex.waiting(), 0);
    let profile = kernel.local.handoff_profile.borrow();
    let counts = profile.mutex();
    ::core::assert_eq!(counts.calls(), 1);
    ::core::assert_eq!(counts.acquired(), 1);
    ::core::assert_eq!(counts.failed_calls(), 0);
    ::core::assert_eq!(counts.immediate(), 0);
    ::core::assert_eq!(counts.recheck(), 0);
    ::core::assert_eq!(counts.queued(), 1);
    ::core::assert_eq!(counts.completed(), 1);
    ::core::assert_eq!(counts.dropped(), 0);
    ::core::assert_eq!(counts.parks(), 1);
    ::core::assert_eq!(counts.park_returns(), 1);
    ::core::assert_eq!(counts.same_owner(), 1);
    ::core::assert_eq!(counts.attempts(), 1);
    drop(profile);
    shared.finish_scope(scope);
}
