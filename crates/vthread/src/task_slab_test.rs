use super::{TaskKey, TaskSlab};

#[test]
fn vacant_slots_are_reused_without_moving_other_values() {
    let mut slab = TaskSlab::new();
    let vacant = slab.insert(String::from("vacant"));
    let retained = slab.insert(String::from("retained"));
    let retained_address = std::ptr::from_ref(slab.get(retained).expect("retained"));

    assert_eq!(slab.remove(vacant).as_deref(), Some("vacant"));
    assert_eq!(slab.insert(String::from("replacement")), vacant);
    assert_eq!(
        std::ptr::from_ref(slab.get(retained).expect("retained")),
        retained_address
    );
    assert_eq!(std::mem::size_of::<TaskKey>(), std::mem::size_of::<usize>());
    assert_eq!(
        std::mem::size_of::<Option<TaskKey>>(),
        std::mem::size_of::<usize>()
    );
    let owned = TaskKey::owned(retained);
    let borrowed = TaskKey::borrowed(retained);
    assert_eq!(owned.index(), retained);
    assert_eq!(borrowed.index(), retained);
    assert!(!owned.is_borrowed());
    assert!(borrowed.is_borrowed());
    assert_ne!(owned, borrowed);
    assert_eq!(slab.len(), 2);
}
