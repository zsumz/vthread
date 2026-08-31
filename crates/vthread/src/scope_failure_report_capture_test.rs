use super::*;
use crate::{PanicReport, ScopeFailure};
use std::sync::Arc;

#[test]
fn io_reports_keep_safe_metadata_without_capturing_the_error_source() {
    let error = Error::io(
        "open file",
        "é".repeat(1024),
        std::io::Error::from_raw_os_error(2),
    );
    let report = FailureReport::capture(&error);
    assert_eq!(report.kind(), FailureKind::Io);
    assert_eq!(report.operation(), Some("open file"));
    assert_eq!(report.context().unwrap().len(), 256);
    assert!(report.truncated());
    assert_eq!(report.io_kind(), Some(std::io::ErrorKind::NotFound));
    assert_eq!(report.raw_os_error(), Some(2));
}

#[test]
fn task_and_configuration_text_are_bounded_even_for_public_error_variants() {
    let error = Error::TaskPanicked {
        task: crate::TaskId::new(7),
        name: "é".repeat(8192),
        panic: PanicReport::capture(Box::new("panic")),
    };
    let report = FailureReport::capture(&error);
    assert!(report.message().len() <= 1024);
    assert!(report.truncated());
    assert!(report.message().is_char_boundary(report.message().len()));
}

#[test]
fn nested_scope_sanitization_has_a_fixed_primary_path_budget() {
    let mut error = Error::io(
        "read",
        "nested context",
        std::io::Error::from_raw_os_error(2),
    );
    for _ in 0..16 {
        let mut failure = ScopeFailure::default();
        failure.child_failed(error);
        failure.child_failed(Error::WouldBlock);
        error = Error::ScopeFailed(Arc::new(failure));
    }
    let report = FailureReport::capture(&error);
    assert_eq!(report.kind(), FailureKind::ScopeFailed);
    assert_eq!(report.nested_scopes(), 8);
    assert_eq!(report.nested_secondary_failures(), 8);
    assert!(report.truncated());
    assert!(format!("{report:?}").len() < 1024);
}

#[test]
fn shallow_nested_scope_reports_preserve_inert_io_context_and_secondary_counts() {
    let mut failure = ScopeFailure::default();
    failure.child_failed(Error::io(
        "read",
        "nested context",
        std::io::Error::from_raw_os_error(2),
    ));
    failure.child_failed(Error::Cancelled);
    let error = Error::ScopeFailed(Arc::new(failure));
    let report = FailureReport::capture(&error);
    assert_eq!(report.kind(), FailureKind::Io);
    assert_eq!(report.context(), Some("nested context"));
    assert_eq!(report.nested_scopes(), 1);
    assert_eq!(report.nested_secondary_failures(), 1);
    assert!(!report.truncated());
}

#[test]
fn nested_custom_sources_are_not_formatted_or_owned_by_the_report() {
    struct Cause(Arc<std::sync::atomic::AtomicBool>);
    impl std::fmt::Debug for Cause {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("arbitrary nested Debug")
        }
    }
    impl std::fmt::Display for Cause {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("arbitrary nested Display")
        }
    }
    impl std::error::Error for Cause {}
    impl Drop for Cause {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut failure = ScopeFailure::default();
    failure.child_failed(std::io::Error::other(Cause(Arc::clone(&dropped))).into());
    let error = Error::ScopeFailed(Arc::new(failure));
    let report = FailureReport::capture(&error);
    assert_eq!(report.kind(), FailureKind::Io);
    assert_eq!(report.io_kind(), Some(std::io::ErrorKind::Other));
    drop(error);
    assert!(dropped.load(std::sync::atomic::Ordering::Relaxed));
    assert!(format!("{report:?}").contains("source text omitted"));
}
