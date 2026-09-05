use super::{BorrowedTask, KernelTasks, OwnedTask};
use crate::task_slab::TaskKey;

#[test]
fn borrowed_count_tracks_live_slots_independently_of_task_records() {
    let mut tasks = KernelTasks::new();
    assert_eq!(tasks.borrowed_count(), 0);
    // Empty record/fiber fields also occur during owner-only reclamation; the
    // slot remains live until remove, regardless of its intermediate contents.
    let owned = tasks.owned.insert(OwnedTask {
        fiber: None,
        execution: None,
    });
    let first = tasks.borrowed.insert(BorrowedTask {
        fiber: None,
        execution: None,
    });
    let second = tasks.borrowed.insert(BorrowedTask {
        fiber: None,
        execution: None,
    });
    assert_eq!(tasks.borrowed_count(), 2);
    assert!(tasks.remove(TaskKey::owned(owned)));
    assert_eq!(tasks.borrowed_count(), 2);
    assert!(tasks.remove(TaskKey::borrowed(first)));
    assert!(!tasks.remove(TaskKey::borrowed(first)));
    assert_eq!(tasks.borrowed_count(), 1);
    let reused = tasks.borrowed.insert(BorrowedTask {
        fiber: None,
        execution: None,
    });
    assert_eq!(reused, first);
    assert_eq!(tasks.borrowed_count(), 2);
    assert!(tasks.remove(TaskKey::borrowed(second)));
    assert!(tasks.remove(TaskKey::borrowed(reused)));
    assert_eq!(tasks.borrowed_count(), 0);
    assert!(tasks.is_empty());
}

#[cfg(not(feature = "runtime-evidence"))]
#[test]
fn owned_task_fits_one_cache_line() {
    assert_eq!(std::mem::size_of::<OwnedTask>(), 64);
}
