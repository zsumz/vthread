use super::{EvidenceWakeCause, WakeOrigin};

#[test]
fn wake_values_are_copyable() {
    let origin = WakeOrigin::Carrier(crate::CarrierId(2));
    core::assert_eq!(origin, origin);
    core::assert_eq!(EvidenceWakeCause::Ready, EvidenceWakeCause::Ready);
}
