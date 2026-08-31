use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn claim_restores_before_join_and_commits_before_reporting() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let slot = JoinSlot::new(thread::spawn(|| {}));
    for phase in [1, 6] {
        shared
            .coordinator_fault
            .store(phase, std::sync::atomic::Ordering::Release);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                slot.join(&Arc::downgrade(&shared), ThreadComponent::Carrier);
            }))
            .is_err()
        );
        assert_eq!(slot.joined(), phase == 6);
    }
    shared
        .coordinator_fault
        .store(0, std::sync::atomic::Ordering::Release);
    slot.join(&Arc::downgrade(&shared), ThreadComponent::Carrier);
    assert!(slot.joined());
}

#[test]
fn post_join_fault_cannot_destroy_an_opaque_worker_panic_on_the_join_caller() {
    struct Hostile(std::sync::mpsc::SyncSender<()>);
    impl Drop for Hostile {
        fn drop(&mut self) {
            let _ = self.0.send(());
            panic!("opaque payload destruction escaped quarantine");
        }
    }
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    shared
        .coordinator_fault
        .store(6, std::sync::atomic::Ordering::Release);
    let (dropped, observed) = std::sync::mpsc::sync_channel(1);
    let payload = Hostile(dropped);
    let slot = JoinSlot::new(thread::spawn(move || std::panic::panic_any(payload)));
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            slot.join(&Arc::downgrade(&shared), ThreadComponent::Carrier);
        }))
        .is_err()
    );
    assert!(slot.joined());
    assert!(
        observed.try_recv().is_err(),
        "join caller destroyed the opaque payload"
    );
    assert_eq!(crate::signal::lock(&shared.failures).entries().len(), 1);
}
