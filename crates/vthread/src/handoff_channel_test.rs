use super::{ChannelCall, ChannelDirection, notified};
use crate::{
    CarrierId, CarrierStatus, Runtime, RuntimeConfig,
    control::Shared,
    handoff_profile::HandoffStage,
    kernel::Kernel,
    local_carrier::LocalCarrier,
    wait::{NotifyResult, WaitHub},
};
use std::{rc::Rc, sync::Arc};

#[test]
fn calls_partition_transfers_retries_error_exits_and_unwind() {
    let local = Rc::new(LocalCarrier::new(RuntimeConfig::default()));
    let hub = Arc::new(WaitHub::new(2, Arc::default()));
    let _route = crate::context::mount_carrier(&hub, &local);
    {
        let mut call = ChannelCall::new(ChannelDirection::Send);
        call.success();
    }
    {
        let mut call = ChannelCall::new(ChannelDirection::Send);
        call.park();
        call.returned();
        call.miss();
        call.park();
        call.returned();
        call.success();
    }
    {
        let mut call = ChannelCall::new(ChannelDirection::Send);
        call.park();
        call.returned();
    }
    ::core::assert!(
        std::panic::catch_unwind(|| {
            let _call = ChannelCall::new(ChannelDirection::Send);
            ::core::panic!("incomplete channel call");
        })
        .is_err()
    );
    for outcome in [
        NotifyResult::Woke,
        NotifyResult::Stored,
        NotifyResult::Stored,
        NotifyResult::Closed,
    ] {
        notified(ChannelDirection::Send, outcome, false);
    }
    let profile = local.handoff_profile.borrow();
    let counts = profile.channel(ChannelDirection::Send);
    ::core::assert_eq!(counts.calls(), 4);
    ::core::assert_eq!(counts.transfers(), 2);
    ::core::assert_eq!(counts.failed_calls(), 2);
    ::core::assert_eq!(counts.park_calls(), 3);
    ::core::assert_eq!(counts.park_returns(), 3);
    ::core::assert_eq!(counts.retries(), 1);
    ::core::assert_eq!(counts.resumed_transfers(), 1);
    ::core::assert_eq!(counts.resumed_exits(), 1);
    ::core::assert_eq!(counts.selected_notifications(), 1);
    ::core::assert_eq!(counts.stored_notifications(), 2);
    ::core::assert_eq!(counts.closed_notifications(), 1);
    ::core::assert_eq!(counts.ineligible_notifications(), 4);
}

#[test]
fn real_channel_records_useless_wake_repark_and_disconnection_exit() {
    let config = Runtime::builder()
        .carriers(1)
        .max_vthreads(3)
        .stack_cache_capacity(3)
        .carrier_queue_capacity(3)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let (sender, receiver) = crate::channel::bounded_with_wait_capacity::<u8>(2, 3).unwrap();
    for _ in 0..3 {
        let receiver = receiver.clone();
        shared
            .submit(scope, "channel profile".into(), move || receiver.recv())
            .unwrap();
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    let _route = crate::context::mount_carrier(&kernel.inbox.hub, &kernel.local);
    kernel.receive();
    while kernel.tick(false).unwrap() {}
    sender.try_send(1).unwrap();
    sender.try_send(2).unwrap();
    while kernel.tick(false).unwrap() {}
    sender.close();
    while kernel.tick(false).unwrap() {}
    shared.finish_scope(scope);
    kernel.publish(CarrierStatus::Stopped);
    let snapshot = shared.snapshot();
    let profile = snapshot.carriers()[0].handoff_profile();
    let counts = profile.channel(ChannelDirection::Receive);
    ::core::assert_eq!(counts.calls(), 3);
    ::core::assert_eq!(counts.transfers(), 2);
    ::core::assert_eq!(counts.failed_calls(), 1);
    ::core::assert!(counts.ineligible_notifications() >= 1);
    ::core::assert!(counts.retries() >= 1);
    ::core::assert_eq!(counts.parks(), kernel.stats.parks);
    ::core::assert_eq!(
        counts.park_returns(),
        counts.retries() + counts.resumed_transfers() + counts.resumed_exits()
    );
    ::core::assert_eq!(counts.resumed_exits(), 1);
    ::core::assert!(profile.duration(HandoffStage::ChannelLock).count() >= 5);
    ::core::assert_eq!(
        profile.duration(HandoffStage::ChannelLock).count(),
        profile.duration(HandoffStage::ChannelHeld).count()
    );
    ::core::assert!(profile.duration(HandoffStage::WakePublication).count() >= 3);
    ::core::assert!(profile.local_publications() >= 3);
}
