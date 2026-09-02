use super::{EventSequence, StackId, WaitKey};

#[test]
fn opaque_id_accessors_preserve_runtime_values() {
    let wait = WaitKey::from_token(vthread_stack::ParkToken::new(7, 11));
    core::assert_eq!(wait.wait(), 7);
    core::assert_eq!(wait.generation(), 11);

    let carrier = crate::CarrierId(2);
    let stack = StackId::new(carrier, 19);
    core::assert_eq!(stack.carrier(), carrier);
    core::assert_eq!(stack.local(), 19);
    core::assert_eq!(EventSequence::new(23).get(), 23);
}
