use std::{cell::Cell, rc::Rc};

use super::Execution;
use crate::{Fiber, FiberState, MappedStack, Resume, SuspendError, Suspension, mount::CoreMount};

struct CountDrop(Rc<Cell<usize>>);

impl Drop for CountDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

struct SuspendOnDrop {
    drops: Rc<Cell<usize>>,
    outcome: Rc<Cell<Option<Result<Resume, SuspendError>>>>,
}

impl Drop for SuspendOnDrop {
    fn drop(&mut self) {
        self.outcome.set(Some(crate::suspend(Suspension::YieldNow)));
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn forced_reclamation_rejects_destructor_suspension_and_reuses_the_stack() {
    let stack = MappedStack::new(128 * 1024, 0).unwrap();
    let address = stack.limit();
    let drops = Rc::new(Cell::new(0));
    let outcome = Rc::new(Cell::new(None));
    let outer = CountDrop(Rc::clone(&drops));
    let inner = SuspendOnDrop {
        drops: Rc::clone(&drops),
        outcome: Rc::clone(&outcome),
    };
    // SAFETY: the entry borrows nothing and is reclaimed before this test returns.
    let mut execution = unsafe {
        Execution::start(stack, move || {
            let _outer = outer;
            let _inner = inner;
            crate::suspend(Suspension::YieldNow).unwrap();
        })
    };

    {
        let _mount = CoreMount::install(execution.core_ptr());
        assert_eq!(
            execution.resume(Resume::Continue),
            FiberState::Suspended(Suspension::YieldNow)
        );
        execution.force_unwind();
    }

    assert_eq!(outcome.get(), Some(Err(SuspendError::Panicking)));
    assert_eq!(drops.get(), 2);
    assert!(execution.is_complete());
    let stack = execution.into_stack();
    assert_eq!(stack.limit(), address);

    let mut reused = Fiber::new(stack, || {});
    assert_eq!(reused.resume(), FiberState::Complete);
    assert_eq!(reused.into_stack().limit(), address);
}
