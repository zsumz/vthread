use super::Kernel;
use crate::{CarrierId, Runtime, control::Shared};
use std::{rc::Rc, sync::Arc};

#[test]
fn completed_execution_storage_is_reset_and_reused() {
    let config = Runtime::builder()
        .max_vthreads(1)
        .carrier_queue_capacity(1)
        .stack_cache_capacity(1)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));

    let first_scope = shared.begin_scope().unwrap();
    shared.submit(first_scope, "first".into(), || ()).unwrap();
    kernel.receive();
    let first = *kernel.ready.front().unwrap();
    let address = Rc::as_ptr(kernel.task(first).execution());
    assert!(kernel.tick(true).unwrap());
    assert_eq!(kernel.execution_cache.len(), 1);
    shared.finish_scope(first_scope);

    let second_scope = shared.begin_scope().unwrap();
    shared
        .submit(second_scope, "second".into(), crate::checkpoint)
        .unwrap();
    kernel.receive();
    let second = *kernel.ready.front().unwrap();
    assert_eq!(Rc::as_ptr(kernel.task(second).execution()), address);
    assert!(kernel.tick(true).unwrap());
    shared.finish_scope(second_scope);
}
