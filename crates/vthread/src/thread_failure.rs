//! Bounded terminal failures of owned runtime threads.

use crate::{PanicReport, control::Shared};
use std::{sync::Weak, thread};

/// Which owned runtime component failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThreadComponent {
    /// Affine stack carrier.
    Carrier,
    /// Explicit native blocking worker.
    NativeWorker,
    /// zio readiness driver.
    Readiness,
    /// Per-runtime shutdown coordinator.
    Coordinator,
}

/// Boundary at which an owned thread failure was observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailurePhase {
    /// Unexpected failure outside an ordinary task or job panic boundary.
    Running,
    /// Destruction of a caught panic payload failed.
    PanicCleanup,
    /// Joining the OS thread returned a panic payload.
    Join,
}

/// One bounded component failure; ordinary task panics are reported by their handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadFailure {
    component: ThreadComponent,
    name: String,
    phase: FailurePhase,
    panic: PanicReport,
    cleanup_complete: bool,
}

impl ThreadFailure {
    pub(crate) fn new(
        component: ThreadComponent,
        name: &str,
        phase: FailurePhase,
        panic: PanicReport,
    ) -> Self {
        let mut end = name.len().min(128);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            component,
            name: name[..end].to_owned(),
            phase,
            cleanup_complete: phase == FailurePhase::Join && !panic.cleanup_panicked(),
            panic,
        }
    }
    /// Component that failed.
    pub fn component(&self) -> ThreadComponent {
        self.component
    }
    /// OS worker name, including its runtime-local index where applicable.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Failure observation boundary.
    pub fn phase(&self) -> FailurePhase {
        self.phase
    }
    /// Bounded captured panic text and disposal status.
    pub fn panic(&self) -> &PanicReport {
        &self.panic
    }
    /// Whether a join confirmed OS cleanup without a panic-payload disposal failure.
    pub fn cleanup_complete(&self) -> bool {
        self.cleanup_complete
    }
}

/// Bounded retained terminal failures; further failures contribute to a counter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadFailures {
    entries: Vec<ThreadFailure>,
    additional: u64,
}

impl ThreadFailures {
    pub(crate) fn joined(&mut self, component: ThreadComponent, name: &str) {
        for entry in &mut self.entries {
            if entry.component == component && entry.name == name {
                entry.cleanup_complete = !entry.panic.cleanup_panicked();
            }
        }
    }
    pub(crate) fn push(&mut self, failure: ThreadFailure) {
        if self.entries.len() < 8 {
            self.entries.push(failure);
        } else {
            self.additional = self.additional.saturating_add(1);
        }
    }
    /// First eight component failures in observation order.
    pub fn entries(&self) -> &[ThreadFailure] {
        &self.entries
    }
    /// Number of further failures omitted from the retained list.
    pub fn additional(&self) -> u64 {
        self.additional
    }
    /// Whether any terminal failure was observed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.additional == 0
    }
}

pub(crate) fn join(
    worker: thread::JoinHandle<()>,
    shared: &Weak<Shared>,
    component: ThreadComponent,
) {
    let name = worker.thread().name().unwrap_or("unnamed").to_owned();
    if let Err(payload) = worker.join() {
        let panic = PanicReport::from_captured(
            vthread_stack::panic_payload::capture_without_observer(payload),
        );
        if let Some(shared) = shared.upgrade() {
            shared.record_failure(ThreadFailure::new(
                component,
                &name,
                FailurePhase::Join,
                panic,
            ));
        }
    } else if let Some(shared) = shared.upgrade() {
        crate::signal::lock(&shared.failures).joined(component, &name);
    }
}

#[cfg(test)]
#[path = "thread_failure_test.rs"]
mod thread_failure_test;
