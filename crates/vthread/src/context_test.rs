use std::sync::Arc;

use crate::{TaskId, wait::WaitHub};

use super::{current, mount};

#[test]
fn mounts_restore_the_previous_task() {
    let hub = Arc::new(WaitHub::new(64, Arc::default()));
    assert!(current().is_none());
    {
        let _outer = mount(TaskId::new(1), Arc::clone(&hub));
        assert_eq!(current().expect("outer task").task_id(), TaskId::new(1));
        {
            let _inner = mount(TaskId::new(2), Arc::clone(&hub));
            assert_eq!(current().expect("inner task").task_id(), TaskId::new(2));
        }
        assert_eq!(current().expect("outer restored").task_id(), TaskId::new(1));
    }
    assert!(current().is_none());
}
