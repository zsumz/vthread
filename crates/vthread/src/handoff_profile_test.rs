use super::{HANDOFF_DURATION_BOUNDS_NS, HandoffDuration, HandoffProfile, HandoffStage};
use std::time::Duration;

#[test]
fn duration_bins_include_boundaries_zero_and_saturating_large_durations() {
    let mut duration = HandoffDuration::default();
    duration.record(Duration::ZERO);
    for bound in HANDOFF_DURATION_BOUNDS_NS {
        duration.record(Duration::from_nanos(bound));
        if bound != u64::MAX {
            duration.record(Duration::from_nanos(bound + 1));
        }
    }
    ::core::assert_eq!(duration.count(), 20);
    ::core::assert_eq!(duration.bins(), &[2; 10]);
    ::core::assert_eq!(duration.maximum_ns(), u64::MAX);
    ::core::assert_eq!(duration.total_ns(), u64::MAX);
    duration.record(Duration::MAX);
    ::core::assert_eq!(duration.count(), 21);
    ::core::assert_eq!(duration.bins()[9], 3);
    ::core::assert_eq!(duration.total_ns(), u64::MAX);
}

#[test]
fn every_stage_has_independent_bounded_storage() {
    let mut profile = HandoffProfile::default();
    for (index, stage) in HandoffStage::ALL.into_iter().enumerate() {
        ::core::assert_eq!(stage as usize, index);
        profile.record(stage, Duration::from_nanos(index as u64));
    }
    for (index, stage) in HandoffStage::ALL.into_iter().enumerate() {
        ::core::assert_eq!(profile.duration(stage).count(), 1);
        ::core::assert_eq!(profile.duration(stage).total_ns(), index as u64);
    }
    ::core::assert!(std::mem::size_of::<HandoffProfile>() <= 2048);
}
