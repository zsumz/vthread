use crate::{ParkOutcome, Runtime, StallPolicy, park_pair, support_test::until};
use std::{sync::Arc, time::Duration};

#[test]
fn report_only_keeps_children_owned_until_an_external_wake() {
    let runtime = Runtime::builder()
        .stall_policy(StallPolicy::ReportAfter(Duration::from_millis(10)))
        .build()
        .unwrap();
    let (parker, wake) = park_pair();
    let shared = Arc::clone(&runtime.shared);
    let observer = std::thread::spawn(move || {
        until(|| shared.snapshot().last_stall.is_some());
        let snapshot = shared.snapshot();
        assert_eq!(snapshot.active, 1);
        assert_eq!(snapshot.stats.aborted, 0);
        assert!(snapshot.accepting);
        wake.unpark();
    });
    runtime
        .run_scope(|scope| {
            assert_eq!(
                scope
                    .spawn("legitimate wait", move || parker.park())?
                    .join()??,
                ParkOutcome::Ready
            );
            Ok(())
        })
        .unwrap();
    observer.join().unwrap();
    assert!(matches!(
        runtime.snapshot().last_stall.unwrap().policy,
        StallPolicy::ReportAfter(_)
    ));
}
