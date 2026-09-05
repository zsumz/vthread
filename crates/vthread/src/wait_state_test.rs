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

#[test]
fn resource_decisions_preserve_generation_and_independent_flags() {
    for phase in [
        Phase::Idle,
        Phase::Binding,
        Phase::Active,
        Phase::ClaimReady,
        Phase::SelectedReady,
        Phase::ClaimTimedOut,
        Phase::SelectedTimedOut,
        Phase::ClaimCancelled,
        Phase::SelectedCancelled,
        Phase::ClaimInheritedCancelled,
        Phase::SelectedInheritedCancelled,
        Phase::ClaimClosed,
        Phase::SelectedClosed,
    ] {
        for resource in [
            None,
            Some(ResourceSelection::Permit),
            Some(ResourceSelection::Broadcast),
        ] {
            for flags in 0..8 {
                let word = WaitWord::initial()
                    .with_generation(super::MAX_GENERATION)
                    .with_phase(phase)
                    .with_resource(resource)
                    .with_permit(flags & 1 != 0)
                    .with_closed(flags & 2 != 0)
                    .with_fallback_hub(flags & 4 != 0);
                let offered = word.resource_offer(ResourceSelection::Permit);
                let eligible = matches!(phase, Phase::Idle | Phase::Active)
                    && !word.is_closed()
                    && !word.has_permit()
                    && resource.is_none();
                assert_eq!(offered.is_some(), eligible);
                if let Some(offered) = offered {
                    assert_eq!(offered.generation(), word.generation());
                    assert_eq!(offered.uses_fallback_hub(), word.uses_fallback_hub());
                    assert_eq!(offered.resource(), Some(ResourceSelection::Permit));
                    assert_eq!(offered.is_claimed(), phase == Phase::Active);
                    assert_eq!(offered.has_permit(), phase == Phase::Idle);
                }
                let taken = word.resource_take();
                assert_eq!(
                    taken.is_some(),
                    resource.is_some() && phase != Phase::Binding && !word.is_claimed()
                );
                if let Some((next, taken)) = taken {
                    assert_eq!(Some(taken), resource);
                    assert_eq!(next.with_resource(resource), word);
                }
            }
        }
    }
}
