use std::rc::Rc;

use crate::{TaskId, wait::WaitHub};

use super::{current, mount};

#[test]
fn mounts_restore_the_previous_task() {
    let hub = Rc::new(WaitHub::new());
    assert!(current().is_none());
    {
        let _outer = mount(TaskId::new(1), Rc::clone(&hub));
        assert_eq!(current().expect("outer task").task_id(), TaskId::new(1));
        {
            let _inner = mount(TaskId::new(2), Rc::clone(&hub));
            assert_eq!(current().expect("inner task").task_id(), TaskId::new(2));
        }
        assert_eq!(current().expect("outer restored").task_id(), TaskId::new(1));
    }
    assert!(current().is_none());
}
