use std::{
    thread,
    time::{Duration, Instant},
};

pub(crate) fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for test synchronization"
        );
        thread::yield_now();
    }
}
