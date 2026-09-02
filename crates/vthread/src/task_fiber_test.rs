use super::TaskFiber;
use vthread_stack::{Fiber, FiberState, StackPool};

#[test]
fn completed_owned_fibers_return_their_stack_to_the_owner_pool() {
    let mut pool = StackPool::new(128 * 1024, 1);
    #[cfg(feature = "runtime-evidence")]
    let (identity, stack) = pool.acquire_identified().unwrap();
    #[cfg(not(feature = "runtime-evidence"))]
    let stack = pool.acquire().unwrap();
    #[cfg(feature = "runtime-evidence")]
    let mut task = TaskFiber::owned(Fiber::new(stack, || ()), identity);
    #[cfg(not(feature = "runtime-evidence"))]
    let mut task = TaskFiber::owned(Fiber::new(stack, || ()));
    assert_eq!(task.resume(), Some(FiberState::Complete));
    #[cfg(feature = "runtime-evidence")]
    let reclaimed = task.reclaim_stack(&mut pool);
    #[cfg(not(feature = "runtime-evidence"))]
    task.reclaim_stack(&mut pool);
    #[cfg(feature = "runtime-evidence")]
    assert_eq!(reclaimed, (identity, true));
    assert_eq!(pool.snapshot().cached, 1);
}
