use super::{ExclusiveCell, SpinMutex, WakeMailbox};

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

#[test]
fn crate_surface_exposes_bounded_wake_publication() {
    let mailbox = WakeMailbox::new();
    assert!(!mailbox.publish(0));
    assert_eq!(mailbox.pop(), Some(0));
    assert_eq!(mailbox.pop(), None);
    assert_eq!(std::mem::size_of::<WakeMailbox>(), 128);
}
