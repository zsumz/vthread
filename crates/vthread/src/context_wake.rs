//! Same-carrier ready routing without crossing the shared wake inbox.

use super::{CURRENT, MOUNTED_EXECUTION, MountedTask};
use crate::{wait::WaitHub, wait::WakeNotice};
use std::sync::Arc;
use vthread_stack::ParkToken;

pub(crate) fn enqueue_local_wake(hub: &Arc<WaitHub>, notice: WakeNotice) -> bool {
    let queued = CURRENT.with(|current| {
        let current = current.borrow();
        let Some(MountedTask::Execution(execution)) = current.as_ref() else {
            return false;
        };
        if !Arc::ptr_eq(execution.hub(), hub) {
            return false;
        }
        execution.local().push_wake(notice);
        super::set_carrier_runnable(true);
        true
    });
    queued
        || MOUNTED_EXECUTION
            .with(|execution| {
                if !Arc::ptr_eq(execution.hub(), hub) {
                    return false;
                }
                execution.local().push_wake(notice);
                super::set_carrier_runnable(true);
                true
            })
            .unwrap_or(false)
}

pub(crate) fn unregister_local_wake(hub: &Arc<WaitHub>, token: ParkToken) {
    let removed = CURRENT.with(|current| {
        let current = current.borrow();
        let Some(MountedTask::Execution(execution)) = current.as_ref() else {
            return false;
        };
        if !Arc::ptr_eq(execution.hub(), hub) {
            return false;
        }
        execution.local().unregister_wake(token);
        true
    });
    if !removed {
        let _ = MOUNTED_EXECUTION.with(|execution| {
            if Arc::ptr_eq(execution.hub(), hub) {
                execution.local().unregister_wake(token);
            }
        });
    }
}

#[cfg(test)]
#[path = "context_wake_test.rs"]
mod context_wake_test;
