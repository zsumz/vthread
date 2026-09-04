//! Same-carrier ready routing without crossing the shared wake inbox.

use super::{CURRENT, MOUNTED_EXECUTION, MountedTask};
use crate::{local_carrier::LocalCarrier, wait::WaitHub, wait::WakeNotice};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use vthread_stack::ParkToken;

struct CarrierRoute {
    hub: Arc<WaitHub>,
    local: Rc<LocalCarrier>,
}

thread_local! {
    static CARRIER_ROUTE: RefCell<Option<CarrierRoute>> = const { RefCell::new(None) };
}

pub(crate) struct CarrierRouteGuard {
    previous: Option<CarrierRoute>,
}

pub(crate) fn mount_carrier(hub: &Arc<WaitHub>, local: &Rc<LocalCarrier>) -> CarrierRouteGuard {
    let route = CarrierRoute {
        hub: Arc::clone(hub),
        local: Rc::clone(local),
    };
    let previous = CARRIER_ROUTE.with(|current| current.replace(Some(route)));
    CarrierRouteGuard { previous }
}

pub(crate) fn enqueue_local_wake(hub: &Arc<WaitHub>, notice: WakeNotice) -> bool {
    let routed = CARRIER_ROUTE.with(|current| {
        let current = current.borrow();
        let route = current.as_ref()?;
        if !Arc::ptr_eq(&route.hub, hub) {
            return Some(false);
        }
        route.local.push_wake(notice);
        super::set_carrier_runnable(true);
        Some(true)
    });
    routed.unwrap_or_else(|| enqueue_mounted_wake(hub, notice))
}

pub(crate) fn unregister_local_wake(hub: &Arc<WaitHub>, token: ParkToken) {
    let routed = CARRIER_ROUTE.with(|current| {
        let current = current.borrow();
        let Some(route) = current.as_ref() else {
            return false;
        };
        if Arc::ptr_eq(&route.hub, hub) {
            route.local.unregister_wake(token);
        }
        true
    });
    if !routed {
        unregister_mounted_wake(hub, token);
    }
}

fn enqueue_mounted_wake(hub: &Arc<WaitHub>, notice: WakeNotice) -> bool {
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

fn unregister_mounted_wake(hub: &Arc<WaitHub>, token: ParkToken) {
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

impl Drop for CarrierRouteGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CARRIER_ROUTE.with(|current| {
            drop(current.replace(previous));
        });
    }
}

#[cfg(test)]
#[path = "context_wake_test.rs"]
mod context_wake_test;
