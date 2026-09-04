//! Typed failure context without public invariant-string contracts.
use std::{
    fmt, io,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
thread_local! {
    static FAULTS_CREATED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// Bounded resource that rejected admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapacityResource {
    /// Retained task records.
    Tasks,
    /// Root and supervisor scopes.
    Scopes,
    /// Per-carrier start packets.
    CarrierQueue,
    /// Queued, running and discarding native jobs combined.
    NativeJobs,
    /// Per-task context entries, including initializing entries.
    TaskLocals,
    /// Selected and waiting synchronization registrations.
    Waiters,
    /// Outstanding readiness registrations.
    Readiness,
    /// Cancellation subscribers.
    CancellationSubscriptions,
    /// Unfinished runtime shutdown owners.
    Lifecycles,
}
/// Internal subsystem that detected a fault; details are diagnostic, not a recovery contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FaultComponent {
    /// Scheduler or wait-state bookkeeping.
    Scheduler,
    /// Native job bookkeeping.
    Native,
    /// Readiness bookkeeping.
    Readiness,
    /// Runtime lifecycle bookkeeping.
    Lifecycle,
}
/// Opaque fault incident; the internal detail is intentionally not a matchable public field.
#[derive(Debug)]
pub struct RuntimeFault {
    incident_id: u64,
    component: FaultComponent,
    detail: &'static str,
}
impl RuntimeFault {
    pub(crate) fn new(component: FaultComponent, detail: &'static str) -> Self {
        #[cfg(test)]
        FAULTS_CREATED.with(|created| created.set(created.get() + 1));
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let incident_id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("fault identity space exhausted");
        Self {
            incident_id,
            component,
            detail,
        }
    }
    /// Process-local incident identifier for correlating diagnostic output.
    pub fn incident_id(&self) -> u64 {
        self.incident_id
    }
    /// Internal subsystem that detected this incident.
    pub fn component(&self) -> FaultComponent {
        self.component
    }
    #[cfg(test)]
    pub(crate) fn created_on_current_thread() -> u64 {
        FAULTS_CREATED.with(std::cell::Cell::get)
    }
}
impl fmt::Display for RuntimeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "runtime fault {} ({:?}): {}",
            self.incident_id, self.component, self.detail
        )
    }
}
/// I/O cause plus bounded operation and path/socket context.
#[derive(Debug)]
pub struct IoFailure {
    operation: &'static str,
    context: String,
    context_truncated: bool,
    source: io::Error,
}
impl IoFailure {
    pub(crate) fn new(
        operation: &'static str,
        context: impl fmt::Display,
        source: io::Error,
    ) -> Self {
        let mut text = crate::diagnostic_text::BoundedText::new(256);
        let _ = fmt::write(&mut text, format_args!("{context}"));
        Self {
            operation,
            context: text.text,
            context_truncated: text.truncated,
            source,
        }
    }
    /// Operation that failed.
    pub fn operation(&self) -> &str {
        self.operation
    }
    /// Path, socket or backend context, capped at 256 UTF-8 bytes.
    pub fn context(&self) -> &str {
        &self.context
    }
    /// Whether the context exceeded its byte budget.
    pub fn context_truncated(&self) -> bool {
        self.context_truncated
    }
    /// Original platform-independent error kind.
    pub fn kind(&self) -> io::ErrorKind {
        self.source.kind()
    }
    /// Original platform error code, when present.
    pub fn raw_os_error(&self) -> Option<i32> {
        self.source.raw_os_error()
    }
    /// Recovers ownership of the original I/O cause, including any custom source.
    pub fn into_io_error(self) -> io::Error {
        self.source
    }
    /// Original I/O cause.
    pub fn io_error(&self) -> &io::Error {
        &self.source
    }
}
impl fmt::Display for IoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}{}]: {}",
            self.operation,
            self.context,
            if self.context_truncated { "…" } else { "" },
            self.source
        )
    }
}
impl std::error::Error for IoFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
#[cfg(test)]
#[path = "error_context_test.rs"]
mod error_context_test;
