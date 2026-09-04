use std::{cell::Cell, rc::Rc};

use super::{Command, FiberCore, ForcedUnwind, Outcome};
use crate::{Resume, Suspension, entry::ErasedEntry};

fn core() -> FiberCore {
    FiberCore::new(ErasedEntry::new(|| {}))
}

#[test]
fn every_core_receives_a_distinct_nonzero_cookie() {
    let first = core();
    let second = core();
    assert_ne!(first.cookie(), 0);
    assert_ne!(first.cookie(), second.cookie());
}

#[test]
fn commands_and_outcomes_round_trip_through_the_core() {
    let core = core();
    assert_eq!(core.command(), Command::Resume(Resume::Continue));
    core.issue(Command::ForceUnwind(core.cookie()));
    assert_eq!(core.command(), Command::ForceUnwind(core.cookie()));
    core.report(Outcome::Suspended(Suspension::YieldNow));
    assert!(matches!(
        core.take_outcome(),
        Outcome::Suspended(Suspension::YieldNow)
    ));
}

#[test]
fn only_the_matching_forced_unwind_token_is_suppressed() {
    let core = core();
    let own = Box::new(ForcedUnwind::new(core.cookie()));
    assert!(matches!(core.classify(own), Outcome::Unwound));
    let foreign = Box::new(ForcedUnwind::new(core.cookie() + 1));
    assert!(matches!(core.classify(foreign), Outcome::Panicked(_)));
    let user = Box::new("user panic");
    assert!(matches!(core.classify(user), Outcome::Panicked(_)));
}

#[test]
fn the_entry_is_taken_once_and_dropped_when_unused() {
    let drops = Rc::new(Cell::new(0));
    struct Counted(Rc<Cell<u32>>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let captured = Counted(Rc::clone(&drops));
    let core = FiberCore::new(ErasedEntry::new(move || drop(captured)));
    let entry = core.take_entry();
    assert!(entry.is_some());
    assert!(core.take_entry().is_none());
    drop(entry);
    assert_eq!(drops.get(), 1);
}
