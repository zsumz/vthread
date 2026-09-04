//! Differential qualification: identical traces through both engines must agree.

#[path = "differential_engines_test.rs"]
mod engines;

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use crate::{FiberState, MappedStack, ParkRequest, ParkToken, Resume, Suspension};
use engines::{Corosensei, Engine, Native};

#[derive(Clone, Copy, Debug)]
enum Step {
    /// Yield and record the decision the carrier delivered.
    Yield,
    /// Park on a modeled wait generation and record the decision.
    Park(u64, u64),
    /// Hold a drop-counted guard for the rest of the entry.
    Guard,
    /// Yield inside `catch_unwind`, swallowing whatever unwinds through it.
    CaughtYield,
    /// Run a child of the same engine to completion on its own stack.
    Nested,
    /// Panic with the fixed user message.
    Panic,
}

#[derive(Clone, Copy, Debug)]
enum Drive {
    Resume(Resume),
    Drop,
    Extract,
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    State(FiberState),
    /// `true` for the trace's own panic, `false` for an engine assertion.
    Panic(bool),
    Dropped(bool),
    Extracted(Result<u64, ()>),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Observation {
    events: Vec<Event>,
    decisions: Vec<Resume>,
    caught: u32,
    nested: u32,
    drops: u32,
    complete: Vec<bool>,
}

#[derive(Default)]
struct Log {
    drops: Rc<Cell<u32>>,
    decisions: RefCell<Vec<Resume>>,
    caught: Cell<u32>,
    nested: Cell<u32>,
}

struct Guard(Rc<Cell<u32>>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn body<E: Engine>(handle: &E::Handle, steps: &[Step], log: &Log) {
    let mut guards = Vec::new();
    for step in steps {
        match *step {
            Step::Yield => {
                let decision = E::suspend(handle, Suspension::YieldNow);
                log.decisions.borrow_mut().push(decision);
            }
            Step::Park(wait, generation) => {
                let request = ParkRequest::new(ParkToken::new(wait, generation), None);
                let decision = E::suspend(handle, Suspension::Park(request));
                log.decisions.borrow_mut().push(decision);
            }
            Step::Guard => guards.push(Guard(Rc::clone(&log.drops))),
            Step::CaughtYield => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    E::suspend(handle, Suspension::YieldNow)
                }));
                log.caught.set(log.caught.get() + 1);
            }
            Step::Nested => {
                let stack = MappedStack::new(64 * 1024, 9).expect("allocate child stack");
                let mut child = E::start(stack, |child| {
                    E::suspend(child, Suspension::YieldNow);
                });
                E::resume(&mut child, Resume::Continue);
                if E::resume(&mut child, Resume::Continue) == FiberState::Complete {
                    log.nested
                        .set(log.nested.get() + E::into_stack(child).identity() as u32);
                }
            }
            Step::Panic => panic!("trace panic"),
        }
    }
    drop(guards);
}

fn payload_is_ours(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.downcast_ref::<&str>() == Some(&"trace panic")
}

fn run<E: Engine>(steps: &'static [Step], drives: &[Drive], stack: MappedStack) -> Observation {
    let log = Rc::new(Log::default());
    let entry_guard = Guard(Rc::clone(&log.drops));
    let body_log = Rc::clone(&log);
    let mut execution = Some(E::start(stack, move |handle| {
        let _entry_guard = entry_guard;
        body::<E>(handle, steps, &body_log);
    }));
    let mut observation = Observation::default();
    for drive in drives {
        match *drive {
            Drive::Resume(resume) => {
                let running = execution.as_mut().expect("trace resumes a live execution");
                match catch_unwind(AssertUnwindSafe(|| E::resume(running, resume))) {
                    Ok(state) => observation.events.push(Event::State(state)),
                    Err(payload) => observation
                        .events
                        .push(Event::Panic(payload_is_ours(&*payload))),
                }
                observation.complete.push(E::is_complete(running));
            }
            Drive::Drop => {
                let dropped = catch_unwind(AssertUnwindSafe(|| drop(execution.take()))).is_ok();
                observation.events.push(Event::Dropped(dropped));
            }
            Drive::Extract => {
                let taken = execution.take().expect("trace extracts a live execution");
                let extracted = catch_unwind(AssertUnwindSafe(|| E::into_stack(taken)))
                    .map(|stack| stack.identity())
                    .map_err(drop);
                observation.events.push(Event::Extracted(extracted));
            }
        }
    }
    drop(execution);
    observation.decisions = log.decisions.take();
    observation.caught = log.caught.get();
    observation.nested = log.nested.get();
    observation.drops = log.drops.get();
    observation
}

fn stack() -> MappedStack {
    MappedStack::new(128 * 1024, 5).expect("allocate stack")
}

/// Every scenario names the drop count the native engine must produce, so agreement
/// between the engines can never be vacuous.
fn scenarios() -> Vec<(&'static str, &'static [Step], Vec<Drive>, u32)> {
    use Drive::{Drop, Extract, Resume as Go};
    use Resume::{Continue, Interrupt};
    use Step::{CaughtYield, Guard, Nested, Panic, Park, Yield};
    vec![
        (
            "yields then completes",
            &[Yield, Yield],
            vec![Go(Continue); 3],
            1,
        ),
        (
            "parks with a token",
            &[Park(7, 3), Guard],
            vec![Go(Continue), Go(Interrupt), Extract],
            2,
        ),
        (
            "decisions reach the body",
            &[Yield, Yield],
            vec![Go(Continue), Go(Interrupt), Go(Continue), Extract],
            1,
        ),
        (
            "panics after a guard",
            &[Guard, Yield, Panic],
            vec![Go(Continue), Go(Continue), Extract],
            2,
        ),
        (
            "panics before any suspension",
            &[Guard, Panic],
            vec![Go(Continue), Extract],
            2,
        ),
        (
            "dropped while suspended",
            &[Guard, Yield, Yield],
            vec![Go(Continue), Drop],
            2,
        ),
        ("dropped before starting", &[Guard], vec![Drop], 1),
        (
            "caught token is reinjected",
            &[Guard, CaughtYield, Yield],
            vec![Go(Continue), Drop],
            2,
        ),
        (
            "caught token then completion",
            &[Guard, CaughtYield],
            vec![Go(Continue), Drop],
            2,
        ),
        (
            "extracted while suspended",
            &[Guard, Yield],
            vec![Go(Continue), Extract],
            2,
        ),
        (
            "nested child completes",
            &[Nested, Yield],
            vec![Go(Continue), Go(Continue), Extract],
            1,
        ),
    ]
}

#[test]
fn both_engines_agree_on_every_scenario() {
    for (name, steps, drives, drops) in scenarios() {
        let native = run::<Native>(steps, &drives, stack());
        let corosensei = run::<Corosensei>(steps, &drives, stack());
        assert_eq!(
            native.drops, drops,
            "scenario `{name}` ran an unexpected number of drops"
        );
        assert_eq!(
            native, corosensei,
            "scenario `{name}` diverged between engines"
        );
    }
}

#[test]
fn a_reclaimed_stack_serves_the_next_trace_identically() {
    fn chain<E: Engine>() -> (Observation, Observation) {
        let first = run::<E>(
            &[Step::Guard, Step::Yield, Step::Panic],
            &[Drive::Resume(Resume::Continue); 2],
            stack(),
        );
        let stack = MappedStack::new(128 * 1024, 5).expect("allocate stack");
        let mut execution = E::start(stack, |_| {});
        E::resume(&mut execution, Resume::Continue);
        let reused = E::into_stack(execution);
        let second = run::<E>(
            &[Step::Yield],
            &[Drive::Resume(Resume::Continue); 2],
            reused,
        );
        (first, second)
    }
    let native = chain::<Native>();
    let corosensei = chain::<Corosensei>();
    assert_eq!(native, corosensei);
    assert_eq!(
        native.1.events.last(),
        Some(&Event::State(FiberState::Complete))
    );
}
