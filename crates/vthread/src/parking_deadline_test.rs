use crate::{Error, ParkOutcome, Runtime, ScopeOptions, park_pair, support_test::until};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn ready_and_closed_winners_survive_a_later_inherited_deadline() {
    for closed in [false, true] {
        let runtime = Runtime::builder().carriers(1).build().unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut checked = false;
        let result = runtime.run_scope_with(ScopeOptions::default().deadline(deadline), |scope| {
            let (parker, wake) = park_pair();
            let mut caller = scope.spawn("deadline-winner", move || {
                (parker.park(), crate::checkpoint())
            })?;
            until(|| runtime.snapshot().parked == 1);
            let (release, gate) = mpsc::sync_channel(1);
            let (blocked, observed) = mpsc::sync_channel(1);
            let mut barrier = scope.spawn("delayed-resume", move || {
                blocked.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            observed.recv_timeout(Duration::from_secs(5)).unwrap();
            if closed {
                assert!(wake.close());
            } else {
                wake.unpark();
            }
            until(|| Instant::now() >= deadline);
            release.send(()).unwrap();
            let (outcome, checkpoint) = caller.join()?;
            let expected = if closed {
                ParkOutcome::Closed
            } else {
                ParkOutcome::Ready
            };
            assert!(
                matches!(outcome, Ok(winner) if winner == expected),
                "{outcome:?}"
            );
            assert!(matches!(checkpoint, Err(Error::DeadlineExceeded)));
            barrier.join()?;
            checked = true;
            Ok(())
        });
        assert!(checked, "all park outcome assertions must execute");
        assert!(matches!(result, Err(Error::DeadlineExceeded)));
    }
}
