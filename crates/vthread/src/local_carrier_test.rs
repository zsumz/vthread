use super::LocalCarrier;
use crate::RuntimeConfig;

#[test]
fn local_admission_starts_empty_without_allocating_stacks() {
    let carrier = LocalCarrier::new(RuntimeConfig::default());
    assert!(carrier.check_capacity().is_ok());
    assert_eq!(carrier.stacks.borrow().snapshot().allocated, 0);
}
