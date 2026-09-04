//! The engine seam for the differential harness; deliberately not a product abstraction.

use std::{cell::Cell, ptr, rc::Rc};

use crate::{
    FiberState, MappedStack, Resume, Suspension, context::FiberCore, engine_corosensei, mount,
    native,
};

/// Test-only seam over both engines; deliberately not a product abstraction.
pub(super) trait Engine {
    type Execution;
    type Handle: Clone + 'static;
    fn start(stack: MappedStack, entry: impl FnOnce(&Self::Handle) + 'static) -> Self::Execution;
    fn suspend(handle: &Self::Handle, reason: Suspension) -> Resume;
    fn resume(execution: &mut Self::Execution, resume: Resume) -> FiberState;
    fn is_complete(execution: &Self::Execution) -> bool;
    fn into_stack(execution: Self::Execution) -> MappedStack;
}

pub(super) struct Native;

impl Engine for Native {
    type Execution = native::Execution;
    type Handle = Rc<Cell<*const FiberCore>>;

    fn start(stack: MappedStack, entry: impl FnOnce(&Self::Handle) + 'static) -> Self::Execution {
        let handle: Self::Handle = Rc::new(Cell::new(ptr::null()));
        let body = Rc::clone(&handle);
        // SAFETY: every trace entry owns its captures and borrows nothing.
        let execution = unsafe { native::Execution::start(stack, move || entry(&body)) };
        handle.set(execution.yielder());
        execution
    }

    fn suspend(handle: &Self::Handle, reason: Suspension) -> Resume {
        // SAFETY: the handle names the execution whose entry is running on this carrier.
        unsafe { native::suspend(handle.get(), reason) }
    }

    fn resume(execution: &mut Self::Execution, resume: Resume) -> FiberState {
        execution.resume(resume)
    }

    fn is_complete(execution: &Self::Execution) -> bool {
        execution.is_complete()
    }

    fn into_stack(execution: Self::Execution) -> MappedStack {
        execution.into_stack()
    }
}

pub(super) struct Corosensei;

impl Engine for Corosensei {
    type Execution = engine_corosensei::Execution;
    type Handle = ();

    fn start(stack: MappedStack, entry: impl FnOnce(&Self::Handle) + 'static) -> Self::Execution {
        // SAFETY: every trace entry owns its captures and borrows nothing.
        unsafe { engine_corosensei::Execution::start(stack, move || entry(&())) }
    }

    fn suspend((): &Self::Handle, reason: Suspension) -> Resume {
        // The coroutine body mounted its own yielder before running the entry.
        let yielder = mount::mounted_yielder().cast::<engine_corosensei::Yielder>();
        // SAFETY: only a running corosensei entry calls this, so the mount holds its yielder.
        unsafe { engine_corosensei::suspend(yielder, reason) }
    }

    fn resume(execution: &mut Self::Execution, resume: Resume) -> FiberState {
        execution.resume(resume)
    }

    fn is_complete(execution: &Self::Execution) -> bool {
        execution.is_complete()
    }

    fn into_stack(execution: Self::Execution) -> MappedStack {
        execution.into_stack()
    }
}
