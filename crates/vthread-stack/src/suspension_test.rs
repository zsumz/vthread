use super::{ParkToken, Resume, SuspendError};

#[test]
fn park_tokens_expose_their_wait_and_generation() {
    let token = ParkToken::new(7, 3);
    assert_eq!(token.wait(), 7);
    assert_eq!(token.generation(), 3);
    assert!(token < ParkToken::new(7, 4));
}

#[test]
fn the_default_resume_decision_continues() {
    assert_eq!(Resume::default(), Resume::Continue);
}

#[test]
fn the_suspend_error_names_the_missing_mount() {
    assert_eq!(
        SuspendError.to_string(),
        "no virtual-thread stack is mounted on this carrier"
    );
}
