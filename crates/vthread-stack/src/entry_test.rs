use std::{cell::Cell, mem::MaybeUninit, ptr::NonNull, rc::Rc};

use super::ErasedEntry;

struct Counted(Rc<Cell<u32>>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

/// Places `entry` in heap storage that lives as long as the returned box.
fn placed<F: FnOnce()>(entry: F) -> (ErasedEntry, Box<MaybeUninit<F>>) {
    let mut storage = Box::new(MaybeUninit::<F>::uninit());
    let slot = NonNull::from(&mut *storage).cast::<()>();
    // SAFETY: the box is valid and aligned for `F` and outlives the handle in every test.
    let entry = unsafe { ErasedEntry::place(slot, entry) };
    (entry, storage)
}

#[test]
fn a_called_entry_runs_once_and_drops_its_captures_once() {
    let drops = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let captured = Counted(Rc::clone(&drops));
    let body_runs = Rc::clone(&runs);
    let (entry, _storage) = placed(move || {
        let _captured = &captured;
        body_runs.set(body_runs.get() + 1);
    });
    entry.call();
    assert_eq!(runs.get(), 1);
    assert_eq!(drops.get(), 1);
}

#[test]
fn an_uncalled_entry_drops_its_captures_exactly_once_in_place() {
    let drops = Rc::new(Cell::new(0));
    let captured = Counted(Rc::clone(&drops));
    let (entry, _storage) = placed(move || drop(captured));
    drop(entry);
    assert_eq!(drops.get(), 1);
}

#[test]
fn zero_sized_entries_run_from_any_storage() {
    let (entry, _storage) = placed(|| {});
    entry.call();
    let (entry, _storage) = placed(|| {});
    drop(entry);
}
