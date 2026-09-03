use std::ptr;

use super::{ContextKey, ContextSlot, CurrentMount, MountGuard};

static NUMBER: ContextKey<u64> = ContextKey::new();
static OTHER_NUMBER: ContextKey<u64> = ContextKey::new();

#[test]
fn current_mount_is_two_machine_words() {
    assert_eq!(
        std::mem::size_of::<CurrentMount>(),
        2 * std::mem::size_of::<usize>()
    );
}

#[test]
fn context_keys_select_only_their_own_value() {
    let slot = ContextSlot::new(&NUMBER, &17);
    let _mount = MountGuard::install(ptr::null(), Some(&slot));

    assert_eq!(NUMBER.with(|value| *value), Some(17));
    assert!(OTHER_NUMBER.with(|_| ()).is_none());
}
