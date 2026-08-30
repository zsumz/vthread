use crate::{RuntimeStats, StackSnapshot};

#[test]
fn diagnostic_counters_start_at_zero() {
    assert_eq!(RuntimeStats::default().mounts, 0);
    assert_eq!(StackSnapshot::default().cached, 0);
}
