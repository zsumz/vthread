use crate::{ShutdownPhase, control::Shared, signal::lock};
use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

#[test]
fn stop_before_service_publication_still_drains_late_services() {
    let (shared, driver) = late_services();
    let (drained, observe) = mpsc::sync_channel(1);
    *lock(&shared.coordinator_exit_hook) = Some(Box::new(move || {
        let _ = drained.send(());
    }));
    driver.ready(&shared);
    let reached = observe.recv_timeout(Duration::from_secs(5));
    let services = shared.services.get().unwrap();
    let state = (
        shared.shutdown_phase(),
        services.blocking.is_stopped(),
        services.blocking.cleanup_complete(),
        services.reactor.cleanup_complete(),
    );
    // A broken stop protocol must fail, but still release the owned workers.
    if reached.is_err() {
        services.stop();
    }
    let complete = wait_for_completion(&shared);
    assert!(
        reached.is_ok(),
        "drain hook not reached: {reached:?}, {state:?}"
    );
    assert!(
        state.1 && state.2 && state.3,
        "services not drained: {state:?}"
    );
    assert!(
        complete,
        "drained services, incomplete shutdown: {:?}",
        shared.snapshot()
    );
}

#[test]
fn drained_late_services_remain_owned_until_the_coordinator_returns() {
    let (shared, driver) = late_services();
    let (drained, observe) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    *lock(&shared.coordinator_exit_hook) = Some(Box::new(move || {
        let _ = drained.send(());
        // Caller release or sender drop is the only way past this ownership gate.
        let _ = gate.recv();
    }));
    driver.ready(&shared);
    let reached = observe.recv_timeout(Duration::from_secs(5));
    let phase = shared.shutdown_phase();
    let returned = shared.resources.returned.load(Ordering::Acquire);
    let services = shared.services.get().unwrap();
    let drained = services.blocking.cleanup_complete() && services.reactor.cleanup_complete();
    let _ = release.send(());
    if reached.is_err() {
        services.stop();
    }
    let complete = wait_for_completion(&shared);
    assert!(reached.is_ok(), "drain hook not reached: {reached:?}");
    assert!(drained, "late services were not drained before the hook");
    assert_eq!(phase, ShutdownPhase::JoiningNative);
    assert!(!returned, "coordinator returned through a held exit gate");
    assert!(
        complete,
        "released coordinator did not complete: {:?}",
        shared.snapshot()
    );
}

fn late_services() -> (Arc<Shared>, super::ShutdownDriver) {
    let config = crate::RuntimeConfig::default();
    let shared = Arc::new(Shared::new(config));
    let driver = super::ShutdownDriver::new(&shared).unwrap();
    shared.request_stop();
    let services = crate::services::Services::new(config, Arc::downgrade(&shared)).unwrap();
    assert!(!services.blocking.is_stopped());
    assert!(services.reactor.check().is_ok());
    assert!(shared.services.set(services).is_ok());
    (shared, driver)
}

fn wait_for_completion(shared: &Shared) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let observed = shared.changed.version();
        match shared.shutdown_phase() {
            ShutdownPhase::Complete => return true,
            ShutdownPhase::Failed => return false,
            _ => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        shared.changed.wait(observed, Some(deadline));
    }
}
