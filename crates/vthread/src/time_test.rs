use std::time::{Duration, Instant};

use crate::{Runtime, sleep};

#[test]
fn timed_sleep_never_returns_before_the_requested_duration() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("minimum sleep", || {
                    let duration = Duration::from_millis(1);
                    let started = Instant::now();
                    sleep(duration).unwrap();
                    assert!(started.elapsed() >= duration);
                })?
                .join()
        })
        .unwrap();
}
