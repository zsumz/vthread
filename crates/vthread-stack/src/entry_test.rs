use std::{cell::Cell, rc::Rc};

use super::ErasedEntry;

struct Counted(Rc<Cell<u32>>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn a_called_entry_runs_once_and_drops_its_captures_once() {
    let drops = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let captured = Counted(Rc::clone(&drops));
    let body_runs = Rc::clone(&runs);
    let entry = ErasedEntry::new(move || {
        let _captured = &captured;
        body_runs.set(body_runs.get() + 1);
    });
    entry.call();
    assert_eq!(runs.get(), 1);
    assert_eq!(drops.get(), 1);
}

#[test]
fn an_uncalled_entry_drops_its_captures_exactly_once() {
    let drops = Rc::new(Cell::new(0));
    let captured = Counted(Rc::clone(&drops));
    let entry = ErasedEntry::new(move || drop(captured));
    drop(entry);
    assert_eq!(drops.get(), 1);
}

#[test]
fn zero_sized_entries_need_no_allocation_to_run() {
    ErasedEntry::new(|| {}).call();
    drop(ErasedEntry::new(|| {}));
}
