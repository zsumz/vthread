use super::ExclusiveCell;

#[test]
fn crate_surface_exposes_exclusive_value_access() {
    let cell = ExclusiveCell::new(41);
    *cell.try_lock().expect("unlocked cell") += 1;
    assert_eq!(*cell.try_lock().expect("released cell"), 42);
}
