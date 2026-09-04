//! Lifecycle traces: fixed scripts for a fiber and its carrier, each with the exact
//! observation the engine must produce, covering every terminal path it supports.

#[path = "trace_harness_test.rs"]
mod harness;

use crate::{FiberState, ParkRequest, ParkToken, Resume, Suspension};
use harness::{Drive, Event, Observation, Step, expect, run, stack, start};

/// Every drop count includes the guard the entry closure itself captures.
fn scenarios() -> Vec<(&'static str, &'static [Step], Vec<Drive>, Observation)> {
    use Drive::{Drop, Extract, Resume as Go};
    use Event::{Dropped, Extracted, Panic as Panicked, State};
    use FiberState::{Complete, Suspended};
    use Resume::{Continue, Interrupt};
    use Step::{CaughtYield, Guard, Nested, Panic, Park, Yield};
    use Suspension::YieldNow;
    let yielded = || State(Suspended(YieldNow));
    let parked = |wait, generation| {
        let request = ParkRequest::new(ParkToken::new(wait, generation), None);
        State(Suspended(Suspension::Park(request)))
    };
    vec![
        (
            "yields then completes",
            &[Yield, Yield],
            vec![Go(Continue), Go(Continue), Go(Continue), Extract],
            expect(
                vec![yielded(), yielded(), State(Complete), Extracted(Ok(5))],
                vec![Continue, Continue],
                vec![false, false, true],
                1,
            ),
        ),
        (
            "parks with a token and receives an interrupt",
            &[Park(7, 3), Guard],
            vec![Go(Continue), Go(Interrupt), Extract],
            expect(
                vec![parked(7, 3), State(Complete), Extracted(Ok(5))],
                vec![Interrupt],
                vec![false, true],
                2,
            ),
        ),
        (
            "direct and shared suspensions remain ordered",
            &[Yield, Park(9, 4), Yield],
            vec![Go(Continue), Go(Interrupt), Go(Continue), Go(Continue)],
            expect(
                vec![yielded(), parked(9, 4), yielded(), State(Complete)],
                vec![Interrupt, Continue, Continue],
                vec![false, false, false, true],
                1,
            ),
        ),
        (
            "panics after a guard",
            &[Guard, Yield, Panic],
            vec![Go(Continue), Go(Continue), Extract],
            expect(
                vec![yielded(), Panicked(true), Extracted(Ok(5))],
                vec![Continue],
                vec![false, true],
                2,
            ),
        ),
        (
            "panics before any suspension",
            &[Guard, Panic],
            vec![Go(Continue), Extract],
            expect(
                vec![Panicked(true), Extracted(Ok(5))],
                vec![],
                vec![true],
                2,
            ),
        ),
        (
            "dropped while suspended",
            &[Guard, Yield, Yield],
            vec![Go(Continue), Drop],
            expect(vec![yielded(), Dropped(true)], vec![], vec![false], 2),
        ),
        (
            "dropped before starting",
            &[Guard],
            vec![Drop],
            expect(vec![Dropped(true)], vec![], vec![], 1),
        ),
        (
            "caught token is reinjected at the next suspension",
            &[Guard, CaughtYield, Yield],
            vec![Go(Continue), Drop],
            Observation {
                caught: 1,
                ..expect(vec![yielded(), Dropped(true)], vec![], vec![false], 2)
            },
        ),
        (
            "caught token followed by completion",
            &[Guard, CaughtYield],
            vec![Go(Continue), Drop],
            Observation {
                caught: 1,
                ..expect(vec![yielded(), Dropped(true)], vec![], vec![false], 2)
            },
        ),
        (
            "extracted while suspended",
            &[Guard, Yield],
            vec![Go(Continue), Extract],
            expect(vec![yielded(), Extracted(Err(()))], vec![], vec![false], 2),
        ),
        (
            "nested child completes on its own stack",
            &[Nested, Yield],
            vec![Go(Continue), Go(Continue), Extract],
            Observation {
                nested: 9,
                ..expect(
                    vec![yielded(), State(Complete), Extracted(Ok(5))],
                    vec![Continue],
                    vec![false, true],
                    1,
                )
            },
        ),
    ]
}

#[test]
fn every_scenario_produces_its_specified_observation() {
    for (name, steps, drives, expected) in scenarios() {
        let observed = run(steps, &drives, stack());
        assert_eq!(
            observed, expected,
            "scenario `{name}` diverged from its specification"
        );
    }
}

#[test]
fn a_reclaimed_stack_serves_the_next_trace_identically() {
    let first = run(
        &[Step::Guard, Step::Yield, Step::Panic],
        &[Drive::Resume(Resume::Continue); 2],
        stack(),
    );
    assert_eq!(first.drops, 2);
    let mut execution = start(stack(), |_| {});
    execution.resume(Resume::Continue);
    let reused = execution.into_stack();
    let second = run(
        &[Step::Yield],
        &[Drive::Resume(Resume::Continue); 2],
        reused,
    );
    assert_eq!(
        second,
        run(
            &[Step::Yield],
            &[Drive::Resume(Resume::Continue); 2],
            stack()
        )
    );
    assert_eq!(
        second.events.last(),
        Some(&Event::State(FiberState::Complete))
    );
}
