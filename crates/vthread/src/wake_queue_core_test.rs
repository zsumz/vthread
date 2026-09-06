use super::{WakeCause, decode_selection, encode_selection};

#[test]
fn every_packed_cause_preserves_the_maximum_generation() {
    for cause in [
        WakeCause::Ready,
        WakeCause::TimedOut,
        WakeCause::Cancelled,
        WakeCause::InheritedCancelled,
        WakeCause::Closed,
    ] {
        for generation in [1, 41, u64::MAX >> 3] {
            assert_eq!(
                decode_selection(encode_selection(generation, cause)),
                (generation, cause)
            );
        }
    }
}
