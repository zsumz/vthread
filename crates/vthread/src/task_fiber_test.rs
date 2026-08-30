use super::TaskFiber;
use vthread_stack::{Fiber, FiberState, StackPool};

#[test]
fn completed_owned_fibers_return_their_stack_to_the_owner_pool() {
    let mut pool = StackPool::new(128 * 1024, 1);
    let mut task = TaskFiber::Owned(Some(Fiber::new(pool.acquire().unwrap(), || ())));
    assert_eq!(task.resume(), Some(FiberState::Complete));
    task.reclaim_stack(&mut pool);
    assert_eq!(pool.snapshot().cached, 1);
}
