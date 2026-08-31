use super::*;

#[test]
fn returned_and_joined_are_independent_cleanup_requirements() {
    let shared = Shared::new(crate::RuntimeConfig::default());
    let resources = &shared.resources;
    assert!(!resources.drained(&shared, true));
    resources.returned.store(true, Ordering::Release);
    assert!(!resources.drained(&shared, false));
    assert!(resources.drained(&shared, true));
    shared.inboxes[0].started.store(true, Ordering::Release);
    assert!(!resources.drained(&shared, true));
    shared.inboxes[0]
        .scheduler_stopped
        .store(true, Ordering::Release);
    assert!(!resources.drained(&shared, true));
    shared.inboxes[0].reclaimed.store(true, Ordering::Release);
    assert!(resources.drained(&shared, true));
}
