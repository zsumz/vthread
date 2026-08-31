//! Capture only known inert fields; never format or clone an arbitrary error source.

use super::{FailureKind, FailureReport};
use crate::{Error, diagnostic_text::BoundedText};
use std::fmt;

impl FailureReport {
    pub(super) fn new(kind: FailureKind) -> Self {
        Self {
            kind,
            message: format!("{kind:?}"),
            operation: None,
            context: None,
            io_kind: None,
            os_error_code: None,
            truncated: false,
            nested_scopes: 0,
            nested_secondary_failures: 0,
        }
    }

    pub(super) fn capture(mut error: &Error) -> Self {
        let mut depth = 0;
        let mut secondary = 0usize;
        while let Error::ScopeFailed(failure) = error {
            if depth == 8 {
                let mut report = Self::new(FailureKind::ScopeFailed);
                report.truncated = true;
                report.nested_scopes = depth;
                report.nested_secondary_failures = secondary;
                return report;
            }
            depth += 1;
            secondary = secondary.saturating_add(failure.failure_count().saturating_sub(1));
            let Some(primary) = failure.primary() else {
                break;
            };
            error = primary;
        }
        let mut report = Self::capture_leaf(error);
        report.nested_scopes = depth;
        report.nested_secondary_failures = secondary;
        report
    }

    fn capture_leaf(error: &Error) -> Self {
        use FailureKind as K;
        let kind = match error {
            Error::Io(_) => K::Io,
            Error::ReadinessFailed => K::ReadinessFailed,
            Error::BlockingFailed => K::BlockingFailed,
            Error::LimitExceeded { .. } => K::LimitExceeded,
            Error::BlockingPanicked(_) | Error::TaskPanicked { .. } => K::Panicked,
            Error::Closed => K::Closed,
            Error::WouldBlock => K::WouldBlock,
            Error::ResultAlreadyTaken => K::ResultAlreadyTaken,
            Error::Cancelled => K::Cancelled,
            Error::DeadlineExceeded => K::DeadlineExceeded,
            Error::RecursiveTaskLocal => K::RecursiveTaskLocal,
            Error::AllocationFailed { .. } => K::AllocationFailed,
            Error::InvalidConfiguration { .. } => K::InvalidConfiguration,
            Error::Capacity { .. } => K::Capacity,
            Error::RootScopeActive => K::RootScopeActive,
            Error::RuntimeStopped => K::RuntimeStopped,
            Error::ScopeFailed(_) => K::ScopeFailed,
            Error::ShutdownFailed(_) => K::ShutdownFailed,
            Error::LifecycleFailed(_) => K::LifecycleFailed,
            Error::RunFailed(_) => K::RunFailed,
            Error::ConstructionFailed(_) => K::ConstructionFailed,
            Error::InsideVThread | Error::InsideManagedWorker => K::ManagedThread,
            Error::ThreadStart { .. } => K::ThreadStart,
            Error::JoinSelf => K::JoinSelf,
            Error::TaskAborted { .. } => K::TaskAborted,
            Error::OutsideVThread => K::OutsideVThread,
            Error::ParkerBusy => K::ParkerBusy,
            Error::DeadlineOverflow => K::DeadlineOverflow,
            Error::StackAllocation(_) => K::StackAllocation,
            Error::RuntimeStalled { .. } => K::RuntimeStalled,
            Error::Fault(_) => K::Fault,
        };
        let mut report = Self::new(kind);
        match error {
            Error::Io(io) => {
                report.operation = Some(report.copy_text(io.operation(), 128));
                report.context = Some(report.copy_text(io.context(), 256));
                report.truncated |= io.context_truncated();
                report.set_io(io.kind(), io.raw_os_error());
            }
            Error::ThreadStart { component, source } => {
                report.operation = Some(format!("start {component:?}"));
                report.set_io(source.kind(), source.raw_os_error());
            }
            Error::StackAllocation(source) => {
                report.operation = Some("allocate virtual-thread stack".into());
                report.set_io(source.kind(), source.raw_os_error());
            }
            Error::BlockingPanicked(panic) => {
                report.set_message(format_args!(
                    "blocking operation panicked: {}",
                    panic.message()
                ));
                report.truncated |= panic.truncated();
            }
            Error::TaskPanicked { task, name, panic } => {
                report.set_message(format_args!(
                    "task {task} ({name}) panicked: {}",
                    panic.message()
                ));
                report.truncated |= panic.truncated();
            }
            Error::LimitExceeded { resource, limit } => {
                report.set_message(format_args!("{resource:?} limit {limit} exceeded"));
            }
            Error::Capacity { resource, limit } => {
                report.set_message(format_args!("{resource:?} capacity {limit} reached"));
            }
            Error::AllocationFailed {
                resource,
                requested,
            } => {
                report.set_message(format_args!(
                    "cannot reserve {requested} bytes for {resource:?}"
                ));
            }
            Error::InvalidConfiguration { field, message } => {
                report.set_message(format_args!("invalid {field:?}: {message}"));
            }
            Error::TaskAborted { task, reason } => {
                report.set_message(format_args!("task {task} aborted: {reason:?}"));
            }
            Error::RuntimeStalled { active } => {
                report.set_message(format_args!("runtime stalled with {active} live tasks"));
            }
            Error::Fault(fault) => {
                report.set_message(format_args!(
                    "runtime fault {} ({:?})",
                    fault.incident_id(),
                    fault.component()
                ));
            }
            Error::LifecycleFailed(failure) => {
                report.set_message(format_args!(
                    "lifecycle owner failed: {}",
                    failure.panic().message()
                ));
                report.truncated |= failure.panic().truncated();
            }
            _ => {}
        }
        report
    }

    fn copy_text(&mut self, input: &str, limit: usize) -> String {
        let mut text = BoundedText::new(limit);
        let _ = fmt::Write::write_str(&mut text, input);
        self.truncated |= text.truncated;
        text.text
    }

    fn set_message(&mut self, args: fmt::Arguments<'_>) {
        let mut text = BoundedText::new(1024);
        let _ = fmt::write(&mut text, args);
        self.message = text.text;
        self.truncated |= text.truncated;
    }

    fn set_io(&mut self, kind: std::io::ErrorKind, code: Option<i32>) {
        self.io_kind = Some(kind);
        self.os_error_code = code;
        self.set_message(format_args!(
            "I/O error kind={kind:?} os_code={code:?}; source text omitted"
        ));
    }
}

#[cfg(test)]
#[path = "scope_failure_report_capture_test.rs"]
mod scope_failure_report_capture_test;
