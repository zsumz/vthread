use super::{Phase, WaitWord};
use crate::wait::{ResourceSelection, WakeCause};

#[test]
fn state_word_preserves_independent_generation_flags_and_resource() {
    let word = WaitWord::initial()
        .with_generation(42)
        .with_phase(Phase::Active)
        .with_permit(true)
        .with_closed(true)
        .with_fallback_hub(true)
        .with_resource(Some(ResourceSelection::Broadcast));
    assert_eq!(word.generation(), 42);
    assert_eq!(word.phase(), Phase::Active);
    assert!(word.has_permit());
    assert!(word.is_closed());
    assert!(word.uses_fallback_hub());
    assert_eq!(word.resource(), Some(ResourceSelection::Broadcast));
}

#[test]
fn recycle_mask_only_tracks_stored_permits_and_resources() {
    assert!(!WaitWord::initial().needs_recycle());
    assert!(WaitWord::initial().with_permit(true).needs_recycle());
    assert!(
        WaitWord::initial()
            .with_resource(Some(ResourceSelection::Permit))
            .needs_recycle()
    );
    assert!(!WaitWord::initial().with_closed(true).needs_recycle());
    assert!(!WaitWord::initial().with_fallback_hub(true).needs_recycle());
}

#[test]
fn every_winner_has_distinct_claimed_and_published_phases() {
    for cause in [
        WakeCause::Ready,
        WakeCause::TimedOut,
        WakeCause::Cancelled,
        WakeCause::InheritedCancelled,
        WakeCause::Closed,
    ] {
        let claimed = WaitWord::initial().claimed(cause);
        assert!(claimed.is_claimed());
        let selected = claimed
            .with_permit(true)
            .with_closed(true)
            .with_fallback_hub(true)
            .with_resource(Some(ResourceSelection::Broadcast))
            .publish_claim();
        assert_eq!(selected.selected_cause(), Some(cause));
        assert!(!selected.is_claimed());
        assert!(selected.has_permit());
        assert!(selected.is_closed());
        assert!(selected.uses_fallback_hub());
        assert_eq!(selected.resource(), Some(ResourceSelection::Broadcast));
    }
}
