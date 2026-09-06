//! Ordinary-thread ownership for explicitly ordered native policy tests.

use super::Kernel;
use crate::{CarrierId, JoinHandle, Runtime, ScopeOptions, TaskFailure, control::Shared};
use std::{sync::Arc, time::Duration, time::Instant};

pub(super) struct Owner {
    pub(super) kernel: Kernel,
    pub(super) scope: u64,
    pub(super) deadline: Option<Instant>,
}

impl Owner {
    pub(super) fn new(timeout: Option<Duration>) -> Self {
        let config = Runtime::builder()
            .max_vthreads(4)
            .stack_cache_capacity(4)
            .carrier_queue_capacity(4)
            .build()
            .unwrap()
            .config();
        let shared = Arc::new(Shared::new(config));
        let kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
        // Finish cold setup before starting the actual, unchanged policy clock.
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let options = deadline.map_or(ScopeOptions::default(), |deadline| {
            ScopeOptions::default().deadline(deadline)
        });
        let scope = shared.begin_owned(options, false).unwrap();
        Self {
            kernel,
            scope,
            deadline,
        }
    }

    pub(super) fn submit<T: Send + 'static>(
        &self,
        name: &str,
        body: impl FnOnce() -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let shared = &self.kernel.shared;
        let task = shared.submit(self.scope, name.into(), body).unwrap();
        JoinHandle::new(Arc::clone(shared), task.id, task.cell, task.record)
    }

    pub(super) fn tick(&mut self) -> bool {
        let _route = crate::context::mount_carrier(&self.kernel.inbox.hub, &self.kernel.local);
        self.kernel.tick(true).unwrap()
    }

    pub(super) fn drain(&mut self) {
        for _ in 0..16 {
            if !self.tick() {
                return;
            }
        }
        panic!(
            "bounded policy fixture did not drain: {:?}",
            self.kernel.stats
        );
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _route = crate::context::mount_carrier(&self.kernel.inbox.hub, &self.kernel.local);
        while !self.kernel.abort(None, TaskFailure::RuntimeStopped) {
            std::thread::yield_now();
        }
        self.kernel.shared.finish_scope(self.scope);
    }
}

pub(super) fn wait_until(deadline: Instant) {
    // Never mounted: spurious native wakes recheck the real monotonic deadline.
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        std::thread::park_timeout(remaining);
    }
}
