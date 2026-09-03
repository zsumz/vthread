//! Virtual completion waits park on registered generations without blocking the carrier.

use crate::{
    Error, Parker, Result, SuspensionReason, context, task::SharedTaskRecord,
    task_context::TaskContext, wait::WaitCell,
};
use std::{rc::Rc, sync::Arc};

struct WaitGuard {
    data: Rc<TaskContext>,
    reason: SuspensionReason,
    masked: usize,
}
impl Drop for WaitGuard {
    fn drop(&mut self) {
        self.data.replace_reason(self.reason);
        self.data.set_masked(self.masked);
    }
}

pub(crate) fn wait_for(
    record: &SharedTaskRecord,
    reason: SuspensionReason,
    shielded: bool,
) -> Result<()> {
    let mounted = context::current().ok_or(Error::OutsideVThread)?;
    let execution = mounted.execution()?;
    if !shielded {
        execution.data.check()?;
    }
    if mounted.task_id() == record.lock().id && Arc::ptr_eq(execution.record(), record) {
        return Err(Error::JoinSelf);
    }
    let data = Rc::clone(&execution.data);
    let _guard = WaitGuard {
        reason: data.replace_reason(reason),
        masked: data.masked(),
        data: Rc::clone(&data),
    };
    if shielded {
        data.set_masked(data.masked() + 1);
    }
    let parker = Parker {
        wait: WaitCell::new(),
    };
    let _subscription = record.subscribe_completion(&parker.wait)?;
    while !record.completion().done() {
        parker.park()?;
    }
    // Completion commits this wait; later policy belongs to the next boundary.
    Ok(())
}

#[cfg(test)]
#[path = "join_wait_test.rs"]
mod join_wait_test;
