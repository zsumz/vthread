use super::super::WakeCause;
use super::{Phase, WaitWord};

#[test]
fn completion_interest_clears_without_changing_selection_or_generation() {
    for cause in [
        WakeCause::Ready,
        WakeCause::TimedOut,
        WakeCause::Cancelled,
        WakeCause::InheritedCancelled,
        WakeCause::Closed,
    ] {
        let claim = WaitWord::initial()
            .with_generation(u64::MAX >> 9)
            .with_phase(Phase::Active)
            .claimed(cause);
        let watched = claim.with_permit(true);
        assert_eq!(watched.with_permit(false), claim);
        let published = watched.publish_claim().with_permit(false);
        assert_eq!(published.selected_cause(), Some(cause));
        assert_eq!(published.generation(), claim.generation());
        assert!(!published.has_permit());
    }
}
