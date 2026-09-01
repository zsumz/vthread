//! Downstream ownership, diagnostic, and constructor checks using public types only.

use std::{
    cell::Cell,
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use vthread::{
    Error, Result, Runtime, channel,
    error::{FailureKind, ScopeRunError},
    sync::{Condvar, DEFAULT_WAIT_CAPACITY, Mutex, Notify, Semaphore},
};

pub(crate) fn verify() -> Result<()> {
    generic_body_errors()?;
    generic_application_errors();
    caller_owned_io_sources()?;
    default_waiter_budgets()
}

fn generic_application_errors() {
    // The top-level runner preserves the same borrowed, non-Send error behavior.
    let message = String::from("caller-owned application error");
    let error = (&message, Rc::new(Cell::new(42)));
    let result = vthread::try_run(|_| Err::<(), _>(error));
    let Err(failure) = result else {
        panic!("application body failure lost")
    };
    let failure: vthread::error::ApplicationRunFailure<(&String, Rc<Cell<i32>>)> = failure;
    assert_eq!(failure.body().unwrap().0, &message);
    assert!(failure.scope().is_none() && failure.shutdown().is_none());
    let (body, scope, shutdown) = failure.into_parts();
    assert!(scope.is_none() && shutdown.is_none());
    assert_eq!(body.unwrap().1.get(), 42);
    let error = Runtime::builder().blocking_capacity(0).build().unwrap_err();
    assert!(matches!(
        error,
        Error::InvalidConfiguration {
            field: vthread::error::ConfigurationField::BlockingCapacity,
            ..
        }
    ));
}

fn generic_body_errors() -> Result<()> {
    // No Send, Display, Debug, Error, or static-lifetime requirement is imposed.
    struct ApplicationError<'a> {
        message: &'a str,
        dropped: Rc<Cell<bool>>,
    }
    impl Drop for ApplicationError<'_> {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    let runtime = Runtime::new()?;
    let message = String::from("application-owned input failure");
    let dropped = Rc::new(Cell::new(false));
    let result = runtime.try_run_scope(|_| {
        Err::<(), _>(ApplicationError {
            message: &message,
            dropped: Rc::clone(&dropped),
        })
    });
    let Err(ScopeRunError::Body(error)) = result else {
        panic!("domain error lost its original representation");
    };
    assert_eq!(error.message, message);
    let snapshot = runtime.snapshot();
    assert_eq!(
        snapshot
            .last_scope_failure()
            .unwrap()
            .body()
            .unwrap()
            .kind(),
        FailureKind::Application
    );
    drop(error);
    assert!(dropped.get());
    assert!(matches!(
        runtime.run_scope::<()>(|_| Err(Error::WouldBlock)),
        Err(Error::WouldBlock)
    ));
    runtime.run_scope(|scope| {
        scope
            .spawn("borrowed domain error", || {
                let local = String::from("borrowed local error");
                let result = vthread::try_local_scope(|_| Err::<(), _>(&local));
                assert!(matches!(result, Err(ScopeRunError::Body(error)) if error == &local));
            })?
            .join()
    })?;
    runtime.shutdown()?;
    Ok(())
}

struct UnformattableCause(Arc<AtomicBool>);
impl fmt::Display for UnformattableCause {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("runtime diagnostics invoked a user Display implementation");
    }
}
impl fmt::Debug for UnformattableCause {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("runtime diagnostics invoked a user Debug implementation");
    }
}
impl std::error::Error for UnformattableCause {}
impl Drop for UnformattableCause {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn caller_owned_io_sources() -> Result<()> {
    let runtime = Runtime::new()?;
    let dropped = Arc::new(AtomicBool::new(false));
    let source = std::io::Error::other(UnformattableCause(Arc::clone(&dropped)));
    let Err(Error::Io(failure)) = runtime.run_scope::<()>(|_| Err(source.into())) else {
        panic!("a single I/O failure was wrapped or discarded");
    };
    let failure: vthread::error::IoFailure = failure;
    let source = failure.into_io_error();
    assert!(source.get_ref().unwrap().is::<UnformattableCause>());
    let snapshot = runtime.snapshot();
    let scope_report: &vthread::error::ScopeFailureReport = snapshot.last_scope_failure().unwrap();
    let report: &vthread::error::FailureReport = scope_report.body().unwrap();
    assert_eq!(report.kind(), FailureKind::Io);
    assert_eq!(report.io_kind(), Some(std::io::ErrorKind::Other));
    assert_eq!(report.raw_os_error(), None);
    assert!(report.operation().is_some());
    assert!(report.message().len() <= 1024);
    let _ = format!("{snapshot:?}");
    let mut dump = String::new();
    snapshot
        .write_dump(&mut dump)
        .expect("owned String accepts dump writes");
    assert!(dump.contains("last_scope_failure"));
    assert!(!dropped.load(Ordering::Relaxed));
    drop(source);
    assert!(
        dropped.load(Ordering::Relaxed),
        "snapshot retained the custom I/O source"
    );
    runtime.shutdown()?;
    Ok(())
}

fn default_waiter_budgets() -> Result<()> {
    let mutex = Mutex::new(42);
    let condition = Condvar::new();
    let notify = Notify::new();
    let semaphore = Semaphore::new(1)?;
    for capacity in [
        mutex.wait_capacity(),
        condition.wait_capacity(),
        notify.wait_capacity(),
        semaphore.wait_capacity(),
    ] {
        assert_eq!(capacity, DEFAULT_WAIT_CAPACITY);
    }
    let (sender, receiver) = channel::bounded(1)?;
    assert_eq!(sender.capacity(), 1);
    assert_eq!(sender.wait_capacity(), channel::DEFAULT_WAIT_CAPACITY);
    assert_eq!(receiver.wait_capacity(), DEFAULT_WAIT_CAPACITY);
    sender.try_send(42).map_err(|error| error.into_parts().0)?;
    let rejected = sender.try_send(43).unwrap_err();
    assert!(matches!(rejected.error(), Error::WouldBlock));
    assert_eq!(rejected.into_inner(), 43);
    assert_eq!(receiver.try_recv()?, 42);
    assert_eq!(*mutex.try_lock()?, 42);
    let permit = semaphore.try_acquire()?;
    assert!(matches!(semaphore.try_acquire(), Err(Error::WouldBlock)));
    drop(permit);
    notify.notify_one();
    notify.try_notified()?;
    assert_eq!(Mutex::with_wait_capacity((), 3)?.wait_capacity(), 3);
    Ok(())
}

#[cfg(test)]
#[path = "api_checks_test.rs"]
mod api_checks_test;
