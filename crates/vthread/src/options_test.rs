use super::{ScopeOptions, TaskOptions};
use std::time::{Duration, Instant};

#[test]
fn descendants_cannot_extend_an_ancestor_deadline() {
    let early = Instant::now() + Duration::from_secs(1);
    let late = early + Duration::from_secs(1);
    let parent = TaskOptions::root(ScopeOptions::default().deadline(early), 2);
    assert_eq!(parent.child(Some(late)).deadline, Some(early));
    assert_eq!(parent.child(None).deadline, Some(early));
    assert_eq!(
        parent
            .child(Some(early - Duration::from_millis(1)))
            .deadline,
        Some(early - Duration::from_millis(1))
    );
}
