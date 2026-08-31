//! Recognize expected application outcomes without hiding retained scope failures.

use vthread::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServiceFailure {
    Deadline,
    Malformed,
    Disconnected,
}

pub(super) fn expected_service(error: &Error) -> Option<ServiceFailure> {
    scope_body(error, |policy| {
        matches!(policy, Error::Cancelled | Error::DeadlineExceeded)
    })
    .and_then(service_leaf)
}

pub(super) fn expected_shutdown(error: &Error) -> bool {
    scope_body(error, |policy| matches!(policy, Error::Cancelled)).is_some_and(shutdown_leaf)
}

fn service_leaf(error: &Error) -> Option<ServiceFailure> {
    use std::io::ErrorKind;
    match error {
        Error::DeadlineExceeded => Some(ServiceFailure::Deadline),
        Error::Io(source) => match source.kind() {
            ErrorKind::InvalidData => Some(ServiceFailure::Malformed),
            ErrorKind::UnexpectedEof
            | ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected => Some(ServiceFailure::Disconnected),
            _ => None,
        },
        _ => None,
    }
}

fn shutdown_leaf(error: &Error) -> bool {
    matches!(
        error,
        Error::Cancelled
            | Error::RuntimeStopped
            | Error::TaskAborted {
                reason: vthread::diagnostics::TaskFailure::RuntimeStopped,
                ..
            }
    )
}

fn scope_body(error: &Error, policy: fn(&Error) -> bool) -> Option<&Error> {
    let Error::ScopeFailed(failure) = error else {
        return Some(error);
    };
    ScopeParts {
        body: failure.body(),
        policy: failure.policy(),
        cleanup: failure.cleanup(),
        child: failure.child(),
        additional_child: failure.additional_child_failures(),
        additional_cleanup: failure.additional_cleanup_failures(),
        body_panicked: failure.body_panicked(),
    }
    .body_for(policy)
}

// A borrowed view makes every independent failure component explicit and testable.
struct ScopeParts<'a> {
    body: Option<&'a Error>,
    policy: Option<&'a Error>,
    cleanup: Option<&'a Error>,
    child: Option<&'a Error>,
    additional_child: usize,
    additional_cleanup: usize,
    body_panicked: bool,
}
impl<'a> ScopeParts<'a> {
    fn body_for(&self, policy: fn(&Error) -> bool) -> Option<&'a Error> {
        if self.policy.is_some_and(policy)
            && self.cleanup.is_none()
            && self.child.is_none()
            && self.additional_child == 0
            && self.additional_cleanup == 0
            && !self.body_panicked
        {
            self.body
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "failure_test.rs"]
mod failure_test;
