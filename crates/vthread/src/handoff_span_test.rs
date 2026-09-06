use super::{Span, duration};
use crate::{
    CarrierId, Runtime, RuntimeConfig, control::Shared, handoff_profile::HandoffStage,
    kernel::Kernel, local_carrier::LocalCarrier, signal::Signal, wait::WaitHub,
};
use std::{
    rc::Rc,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

#[test]
fn nested_carrier_routes_restore_owner_local_attribution() {
    let first = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
    let second = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    duration(HandoffStage::WakePublication, Duration::from_nanos(100));
    ::core::assert_eq!(
        first
            .handoff_profile
            .borrow()
            .duration(HandoffStage::WakePublication)
            .count(),
        0
    );
    let outer = crate::context::mount_carrier(&hub, &first);
    duration(HandoffStage::WakePublication, Duration::from_nanos(2));
    {
        let _inner = crate::context::mount_carrier(&hub, &second);
        duration(HandoffStage::WakePublication, Duration::from_nanos(3));
    }
    duration(HandoffStage::WakePublication, Duration::from_nanos(5));
    drop(outer);
    duration(HandoffStage::WakePublication, Duration::from_nanos(100));
    ::core::assert_eq!(
        first
            .handoff_profile
            .borrow()
            .duration(HandoffStage::WakePublication)
            .total_ns(),
        7
    );
    ::core::assert_eq!(
        second
            .handoff_profile
            .borrow()
            .duration(HandoffStage::WakePublication)
            .total_ns(),
        3
    );
}

#[test]
fn native_wait_is_counted_only_after_the_predicate_commits_to_it() {
    let signal = Arc::new(Signal::default());
    let observed = signal.version();
    let (entered, waiting) = mpsc::sync_channel(1);
    let worker_signal = Arc::clone(&signal);
    let worker = std::thread::spawn(move || {
        let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
        let hub = Arc::new(WaitHub::new(2, Arc::clone(&worker_signal)));
        let _route = crate::context::mount_carrier(&hub, &local);
        let mut entered = Some(entered);
        // The epoch has already been tested and the gate is held. A notifier
        // cannot take the gate until this selected native wait releases it.
        worker_signal.wait_while(observed, None, || {
            if let Some(entered) = entered.take() {
                entered.send(()).unwrap();
            }
            false
        });
        *local.handoff_profile.borrow()
    });
    let reached = waiting.recv_timeout(Duration::from_secs(5));
    signal.notify();
    let profile = worker.join().unwrap();
    reached.unwrap();
    ::core::assert!(profile.duration(HandoffStage::NativeWait).count() >= 1);
    ::core::assert_eq!(profile.duration(HandoffStage::WaitApi).count(), 0);
}

#[test]
fn control_only_poll_and_due_timer_do_not_claim_task_work_or_native_sleep() {
    let config = Runtime::builder()
        .carriers(2)
        .max_vthreads(2)
        .stack_cache_capacity(2)
        .carrier_queue_capacity(2)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let mut kernel = Kernel::new(shared, CarrierId(0));
    let _route = crate::context::mount_carrier(&kernel.inbox.hub, &kernel.local);
    let observed = kernel.inbox.signal.version();
    kernel.inbox.signal.notify();
    kernel.wait_for_work(observed);
    let token = vthread_stack::ParkToken::new(1, 1);
    ::core::assert!(kernel.timers.schedule(
        crate::task_slab::TaskKey::owned(0),
        token,
        Instant::now()
    ));
    kernel.wait_for_work(kernel.inbox.signal.version());
    ::core::assert!(kernel.timers.cancel(token));
    let profile = *kernel.local.handoff_profile.borrow();
    ::core::assert_eq!(profile.duration(HandoffStage::IdleEpisode).count(), 2);
    ::core::assert_eq!(profile.duration(HandoffStage::PollControl).count(), 1);
    ::core::assert_eq!(profile.duration(HandoffStage::PollWork).count(), 0);
    ::core::assert_eq!(profile.duration(HandoffStage::WaitApi).count(), 1);
    ::core::assert_eq!(profile.duration(HandoffStage::NativeWait).count(), 0);
    ::core::assert_eq!(profile.duration(HandoffStage::IdlePublication).count(), 1);
}

#[test]
fn spans_record_once_on_unwind_without_retaining_a_profile_borrow() {
    let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    let _route = crate::context::mount_carrier(&hub, &local);
    let result = std::panic::catch_unwind(|| {
        let _span = Span::new(HandoffStage::ClaimFinish);
        ::core::panic!("profile unwind probe");
    });
    ::core::assert!(result.is_err());
    let profile = local.handoff_profile.borrow();
    ::core::assert_eq!(profile.duration(HandoffStage::ClaimFinish).count(), 1);
}

#[test]
fn paused_publication_records_both_sides_of_the_real_claim_dependency() {
    use crate::{
        TaskId,
        task_slab::TaskKey,
        wait::{
            WaitBegin, WaitCell, WakeCause,
            wait_publication_probe_test::{PausedPublication, Stage},
        },
    };
    let cell = WaitCell::new();
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    let WaitBegin::Park { request, .. } = cell
        .begin(TaskId::new(1), TaskKey::owned(0), &hub, None)
        .unwrap()
    else {
        ::core::panic!("expected active wait");
    };
    let token = request.token();
    std::thread::scope(|threads| {
        let mut pause = PausedPublication::install(&cell);
        let publisher = threads.spawn(|| {
            let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
            let source = Arc::new(WaitHub::new(2, Arc::default()));
            let _route = crate::context::mount_carrier(&source, &local);
            ::core::assert_eq!(cell.notify(), crate::wait::NotifyResult::Woke);
            *local.handoff_profile.borrow()
        });
        pause.observe(Stage::NoticePublished);
        ::core::assert_eq!(hub.pop_wake().unwrap().token, token);
        let recipient = threads.spawn(|| {
            let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
            let _route = crate::context::mount_carrier(&hub, &local);
            ::core::assert_eq!(cell.finish_plain_ready(token).unwrap(), WakeCause::Ready);
            *local.handoff_profile.borrow()
        });
        pause.observe(Stage::FinishWaiting);
        ::core::assert!(!cell.registration().select_ready(token));
        pause.release();
        let source = publisher.join().unwrap();
        let target = recipient.join().unwrap();
        ::core::assert_eq!(source.duration(HandoffStage::WakePublication).count(), 1);
        ::core::assert_eq!(source.remote_publications(), 1);
        ::core::assert_eq!(target.duration(HandoffStage::ClaimFinish).count(), 1);
    });
}
