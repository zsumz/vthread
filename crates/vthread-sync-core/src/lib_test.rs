use super::{ExclusiveCell, SpinMutex};

#[test]
fn crate_surface_exposes_exclusive_value_access() {
    let cell = ExclusiveCell::new(41);
    *cell.try_lock().expect("unlocked cell") += 1;
    assert_eq!(*cell.try_lock().expect("released cell"), 42);
}

#[test]
fn crate_surface_exposes_short_section_locking() {
    let mutex = SpinMutex::new(41);
    *mutex.lock() += 1;
    assert_eq!(*mutex.lock(), 42);
}
