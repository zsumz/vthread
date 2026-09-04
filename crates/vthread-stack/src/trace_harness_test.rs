//! Trace vocabulary and runner shared by the lifecycle scenarios.

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    rc::Rc,
};

use crate::{
    FiberState, MappedStack, ParkRequest, ParkToken, Resume, Suspension,
    context::FiberCore,
    engine::{Execution, suspend},
};

type Handle = Rc<Cell<*const FiberCore>>;

#[derive(Clone, Copy)]
pub(super) enum Step {
    /// Yield and record the decision the carrier delivered.
    Yield,
    /// Park on a modeled wait generation and record the decision.
    Park(u64, u64),
    /// Hold a drop-counted guard for the rest of the entry.
    Guard,
    /// Yield inside `catch_unwind`, swallowing whatever unwinds through it.
    CaughtYield,
    /// Run a child fiber to completion on its own stack and record its identity.
    Nested,
    /// Panic with the fixed user message.
    Panic,
}

#[derive(Clone, Copy)]
pub(super) enum Drive {
    Resume(Resume),
    Drop,
    Extract,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    State(FiberState),
    /// `true` for the trace's own panic, `false` for an engine assertion.
    Panic(bool),
    Dropped(bool),
    Extracted(Result<u64, ()>),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Observation {
    pub(super) events: Vec<Event>,
    pub(super) decisions: Vec<Resume>,
    pub(super) caught: u32,
    pub(super) nested: u32,
    pub(super) drops: u32,
    pub(super) complete: Vec<bool>,
}

#[derive(Default)]
pub(super) struct Log {
    drops: Rc<Cell<u32>>,
    decisions: RefCell<Vec<Resume>>,
    caught: Cell<u32>,
    nested: Cell<u32>,
}

pub(super) struct Guard(Rc<Cell<u32>>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

pub(super) fn start(stack: MappedStack, entry: impl FnOnce(&Handle) + 'static) -> Execution {
    let handle: Handle = Rc::new(Cell::new(ptr::null()));
    let body = Rc::clone(&handle);
    // SAFETY: every trace entry owns its captures and borrows nothing.
    let execution = unsafe { Execution::start(stack, move || entry(&body)) };
    handle.set(execution.core_ptr());
    execution
}

pub(super) fn park(handle: &Handle, reason: Suspension) -> Resume {
    // SAFETY: the handle names the execution whose entry is running on this carrier.
    unsafe { suspend(handle.get(), reason) }
}

fn body(handle: &Handle, steps: &[Step], log: &Log) {
    let mut guards = Vec::new();
    for step in steps {
        match *step {
            Step::Yield => {
                let decision = park(handle, Suspension::YieldNow);
                log.decisions.borrow_mut().push(decision);
            }
            Step::Park(wait, generation) => {
                let request = ParkRequest::new(ParkToken::new(wait, generation), None);
                let decision = park(handle, Suspension::Park(request));
                log.decisions.borrow_mut().push(decision);
            }
            Step::Guard => guards.push(Guard(Rc::clone(&log.drops))),
            Step::CaughtYield => {
                let _ = catch_unwind(AssertUnwindSafe(|| park(handle, Suspension::YieldNow)));
                log.caught.set(log.caught.get() + 1);
            }
            Step::Nested => {
                let stack = MappedStack::new(64 * 1024, 9).expect("allocate child stack");
                let mut child = start(stack, |child| {
                    park(child, Suspension::YieldNow);
                });
                child.resume(Resume::Continue);
                if child.resume(Resume::Continue) == FiberState::Complete {
                    let identity = child.into_stack().identity();
                    log.nested.set(log.nested.get() + identity as u32);
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

pub(super) fn run(steps: &'static [Step], drives: &[Drive], stack: MappedStack) -> Observation {
    let log = Rc::new(Log::default());
    let entry_guard = Guard(Rc::clone(&log.drops));
    let body_log = Rc::clone(&log);
    let mut execution = Some(start(stack, move |handle| {
        let _entry_guard = entry_guard;
        body(handle, steps, &body_log);
    }));
    let mut observation = Observation::default();
    for drive in drives {
        match *drive {
            Drive::Resume(resume) => {
                let running = execution.as_mut().expect("trace resumes a live execution");
                match catch_unwind(AssertUnwindSafe(|| running.resume(resume))) {
                    Ok(state) => observation.events.push(Event::State(state)),
                    Err(payload) => observation
                        .events
                        .push(Event::Panic(payload_is_ours(&*payload))),
                }
                observation.complete.push(running.is_complete());
            }
            Drive::Drop => {
                let dropped = catch_unwind(AssertUnwindSafe(|| drop(execution.take()))).is_ok();
                observation.events.push(Event::Dropped(dropped));
            }
            Drive::Extract => {
                let taken = execution.take().expect("trace extracts a live execution");
                let extracted = catch_unwind(AssertUnwindSafe(|| taken.into_stack()))
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

pub(super) fn stack() -> MappedStack {
    MappedStack::new(128 * 1024, 5).expect("allocate stack")
}

pub(super) fn expect(
    events: Vec<Event>,
    decisions: Vec<Resume>,
    complete: Vec<bool>,
    drops: u32,
) -> Observation {
    Observation {
        events,
        decisions,
        caught: 0,
        nested: 0,
        drops,
        complete,
    }
}
