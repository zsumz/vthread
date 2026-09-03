use std::{panic::AssertUnwindSafe, rc::Rc, sync::Arc};

use crate::{
    CarrierId, RuntimeConfig, TaskId, control::Shared, kernel::Kernel, kernel_tasks::TaskMut,
    wait::WaitHub,
};

use super::{current, mount, with_execution_slot};

#[test]
fn mounted_execution_hot_state_fits_with_its_rc_header_in_one_cache_line() {
    assert_eq!(std::mem::size_of::<super::Execution>(), 48);
}

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

#[test]
fn slot_mount_restores_execution_ownership_during_unwind() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    shared.submit(scope, "task".into(), || ()).expect("task");
    let mut kernel = Kernel::new(shared, CarrierId(0));
    kernel.receive();
    let key = *kernel.ready.front().expect("ready task");
    let mut task = kernel.tasks.get_mut(key).expect("task slot");
    let slot = match &mut task {
        TaskMut::Owned(task) => &mut task.execution,
        TaskMut::Borrowed(_) => panic!("remote task must be owned"),
    };
    let pointer = Rc::as_ptr(slot.as_ref().expect("execution"));

    let failure = std::panic::catch_unwind(AssertUnwindSafe(|| {
        with_execution_slot(slot, |execution| {
            assert_eq!(Rc::as_ptr(execution), pointer);
            panic!("injected mount failure");
        });
    }));

    assert!(failure.is_err());
    assert_eq!(
        Rc::as_ptr(slot.as_ref().expect("restored execution")),
        pointer
    );
    assert!(current().is_none());
}
