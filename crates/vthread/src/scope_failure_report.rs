//! Inert, bounded scope diagnostics: no caller-owned error sources or callbacks.

#[path = "scope_failure_report_capture.rs"]
mod capture;

/// Stable categories retained without owning the original error value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailureKind {
    /// An application callback returned its own error type.
    Application,
    /// An operating-system I/O operation failed.
    Io,
    /// A readiness service failed.
    ReadinessFailed,
    /// A blocking service failed.
    BlockingFailed,
    /// A configured result limit was exceeded.
    LimitExceeded,
    /// A task or blocking operation panicked.
    Panicked,
    /// A synchronization primitive closed.
    Closed,
    /// A nonblocking operation could not complete.
    WouldBlock,
    /// A terminal result was already consumed.
    ResultAlreadyTaken,
    /// Cancellation was observed.
    Cancelled,
    /// A deadline expired.
    DeadlineExceeded,
    /// Task-local initialization was recursive.
    RecursiveTaskLocal,
    /// A bounded allocation failed.
    AllocationFailed,
    /// Configuration was invalid.
    InvalidConfiguration,
    /// A configured admission limit was reached.
    Capacity,
    /// A root scope was already active.
    RootScopeActive,
    /// The runtime stopped accepting work.
    RuntimeStopped,
    /// A nested scope report exceeded the retained primary-path depth.
    ScopeFailed,
    /// Runtime shutdown retained component failures.
    ShutdownFailed,
    /// The process lifecycle service failed.
    LifecycleFailed,
    /// A managed thread attempted an operation that blocks an OS caller.
    ManagedThread,
    /// An operating-system thread could not start.
    ThreadStart,
    /// A task attempted to join itself.
    JoinSelf,
    /// A child stack was reclaimed without a result.
    TaskAborted,
    /// Suspension was attempted outside a virtual thread.
    OutsideVThread,
    /// A parker already owned a generation.
    ParkerBusy,
    /// A monotonic deadline could not be represented.
    DeadlineOverflow,
    /// A virtual stack could not be allocated.
    StackAllocation,
    /// A configured stall policy observed no progress.
    RuntimeStalled,
    /// An internal runtime fault was recorded.
    Fault,
    /// Both scope execution and runtime shutdown failed.
    RunFailed,
}

/// One sanitized cause. Text is copied only from inert runtime fields, never user formatting.
///
/// Message text is capped at 1024 UTF-8 bytes, operation at 128, and context at 256.
/// Nested scope aggregates retain at most eight primary links and a secondary-failure count;
/// the complete error tree remains owned by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureReport {
    kind: FailureKind,
    message: String,
    operation: Option<String>,
    context: Option<String>,
    io_kind: Option<std::io::ErrorKind>,
    os_error_code: Option<i32>,
    truncated: bool,
    nested_scopes: usize,
    nested_secondary_failures: usize,
}

impl FailureReport {
    /// Category of the retained representative cause.
    pub fn kind(&self) -> FailureKind {
        self.kind
    }
    /// Bounded inert message; custom I/O source messages are intentionally omitted.
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Bounded operation name, when the failure describes I/O.
    pub fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }
    /// Bounded path or peer context, when supplied by the runtime.
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }
    /// Original I/O error kind without retaining the source object.
    pub fn io_kind(&self) -> Option<std::io::ErrorKind> {
        self.io_kind
    }
    /// Original operating-system error code, when available.
    pub fn raw_os_error(&self) -> Option<i32> {
        self.os_error_code
    }
    /// Whether text or nested scope details exceeded a diagnostic bound.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
    /// Nested aggregates traversed to find this representative cause, at most eight.
    pub fn nested_scopes(&self) -> usize {
        self.nested_scopes
    }
    /// Secondary failures counted along the retained nested primary path.
    pub fn nested_secondary_failures(&self) -> usize {
        self.nested_secondary_failures
    }
}

/// Latest failed structured scope, safe for runtime ownership and snapshot formatting.
///
/// At most four causes are retained, each with at most 1408 bytes of inert text.
/// No arbitrary application error, I/O source, formatter, or destructor is retained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeFailureReport {
    body: Option<FailureReport>,
    policy: Option<FailureReport>,
    cleanup: Option<FailureReport>,
    child: Option<FailureReport>,
    additional_child_failures: usize,
    additional_cleanup_failures: usize,
    body_panicked: bool,
}

impl ScopeFailureReport {
    /// Callback failure, or an inert marker for a generic application error.
    pub fn body(&self) -> Option<&FailureReport> {
        self.body.as_ref()
    }
    /// Inherited deadline or cancellation failure.
    pub fn policy(&self) -> Option<&FailureReport> {
        self.policy.as_ref()
    }
    /// First reclamation failure.
    pub fn cleanup(&self) -> Option<&FailureReport> {
        self.cleanup.as_ref()
    }
    /// First unobserved child failure.
    pub fn child(&self) -> Option<&FailureReport> {
        self.child.as_ref()
    }
    /// Number of further unobserved failed children.
    pub fn additional_child_failures(&self) -> usize {
        self.additional_child_failures
    }
    /// Number of further cleanup failures.
    pub fn additional_cleanup_failures(&self) -> usize {
        self.additional_cleanup_failures
    }
    /// Whether the callback unwound and its payload was rethrown.
    pub fn body_panicked(&self) -> bool {
        self.body_panicked
    }
    /// Representative cause in body, inherited policy, cleanup, then child order.
    pub fn primary(&self) -> Option<&FailureReport> {
        self.body()
            .or(self.policy())
            .or(self.cleanup())
            .or(self.child())
    }

    pub(crate) fn capture(failure: &crate::ScopeFailure, application_error: bool) -> Self {
        Self {
            body: failure.body().map(FailureReport::capture).or_else(|| {
                application_error.then(|| FailureReport::new(FailureKind::Application))
            }),
            policy: failure.policy().map(FailureReport::capture),
            cleanup: failure.cleanup().map(FailureReport::capture),
            child: failure.child().map(FailureReport::capture),
            additional_child_failures: failure.additional_child_failures(),
            additional_cleanup_failures: failure.additional_cleanup_failures(),
            body_panicked: failure.body_panicked(),
        }
    }
}

#[cfg(test)]
#[path = "scope_failure_report_test.rs"]
mod scope_failure_report_test;
