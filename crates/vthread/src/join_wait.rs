//! Virtual completion waits park on registered generations without blocking the carrier.

use crate::{
    Error, Parker, Result, SuspensionReason, context, signal::lock, task::SharedTaskRecord,
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
        self.data.reason.set(self.reason);
        self.data.masked.set(self.masked);
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
    if mounted.task_id() == lock(record).id && Arc::ptr_eq(&execution.record, record) {
        return Err(Error::JoinSelf);
    }
    let completion = Arc::clone(&lock(record).completion);
    let data = Rc::clone(&execution.data);
    let _guard = WaitGuard {
        reason: data.reason.replace(reason),
        masked: data.masked.get(),
        data: Rc::clone(&data),
    };
    if shielded {
        data.masked.set(data.masked.get() + 1);
    }
    let parker = Parker {
        wait: WaitCell::new(),
    };
    let _subscription = completion.subscribe(&parker.wait)?;
    while !completion.done() {
        parker.park()?;
    }
    if !shielded {
        execution.data.check()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "join_wait_test.rs"]
mod join_wait_test;
