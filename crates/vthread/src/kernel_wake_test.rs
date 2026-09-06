use crate::{CarrierId, control::Shared, kernel::Kernel};
use std::sync::Arc;

#[test]
fn empty_completion_epochs_do_not_allocate_deferred_storage() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let mut kernel = Kernel::new(shared, CarrierId(0));
    for _ in 0..8 {
        kernel.process_deferred_wakes().unwrap();
    }
    assert_eq!(kernel.deferred_wakes.capacity(), 0);
    assert_eq!(kernel.stats.wakes, 0);
}
