use crate::{Error, Runtime, ScopeFailure, error::FailureKind};
use std::{
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct HostileCause {
    allocation: Arc<Vec<u8>>,
    drops: Arc<AtomicUsize>,
    action: Option<Box<dyn FnOnce() + Send + Sync>>,
}
impl fmt::Display for HostileCause {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("custom Display must not execute during diagnostic capture")
    }
}
impl fmt::Debug for HostileCause {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("custom Debug must not execute during diagnostic capture")
    }
}
impl std::error::Error for HostileCause {}
impl Drop for HostileCause {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
        if let Some(action) = self.action.take() {
            action();
        }
    }
}

#[test]
fn panicking_large_cause_remains_caller_owned_while_retained_reports_are_inert() {
    let runtime = Runtime::new().unwrap();
    let allocation = Arc::new(vec![42; 8 * 1024 * 1024]);
    let retained = Arc::downgrade(&allocation);
    let drops = Arc::new(AtomicUsize::new(0));
    let cause = HostileCause {
        allocation,
        drops: Arc::clone(&drops),
        action: Some(Box::new(|| panic!("caller-owned cause destructor"))),
    };
    // An aggregate exercises both the returned Arc and the runtime-retained report.
    let mut failure = ScopeFailure::default();
    failure.cleanup_failed(Error::Cancelled);
    let error = failure
        .finish::<()>(
            Err(std::io::Error::other(cause).into()),
            Ok(()),
            &runtime.shared,
        )
        .unwrap_err();
    let full = error.scope_failure().unwrap().body().unwrap();
    let Error::Io(io) = full else {
        panic!("original I/O cause was lost")
    };
    assert_eq!(
        io.io_error()
            .get_ref()
            .unwrap()
            .downcast_ref::<HostileCause>()
            .unwrap()
            .allocation
            .len(),
        8 * 1024 * 1024
    );
    let snapshot = runtime.snapshot();
    assert_eq!(
        snapshot
            .last_scope_failure()
            .unwrap()
            .body()
            .unwrap()
            .kind(),
        FailureKind::Io
    );
    let debug = format!("{snapshot:?}");
    let mut dump = String::new();
    snapshot.write_dump(&mut dump).unwrap();
    assert!(debug.contains("source text omitted") && dump.contains("last_scope_failure"));
    assert!(debug.len() < 8192 && dump.len() < 8192);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(error))).is_err());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert!(
        retained.upgrade().is_none(),
        "snapshot retained the arbitrary allocation"
    );
    drop(runtime.run_scope::<()>(|_| Err(Error::WouldBlock)));
    runtime.shutdown().unwrap();
    drop(runtime);
    drop(snapshot);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn blocking_cause_cannot_hold_the_final_managed_runtime_owner_or_later_shutdowns() {
    let runtime = Runtime::new().unwrap();
    let weak = Arc::downgrade(&runtime.shared);
    let drops = Arc::new(AtomicUsize::new(0));
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let gate = Mutex::new(gate);
    let cause = HostileCause {
        allocation: Arc::new(vec![0; 1024]),
        drops: Arc::clone(&drops),
        action: Some(Box::new(move || {
            entered.send(()).unwrap();
            gate.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        })),
    };
    let error = runtime
        .run_scope::<()>(|_| Err(std::io::Error::other(cause).into()))
        .unwrap_err();
    let caller = thread::spawn(move || drop(error));
    let caller_owns_drop = entry.recv_timeout(Duration::from_millis(500)).is_ok();
    let outer = Runtime::new().unwrap();
    let dropped_from_managed =
        outer.run_scope(|scope| scope.spawn("drop runtime", move || drop(runtime))?.join());
    let deadline = Instant::now() + Duration::from_secs(1);
    while weak.strong_count() > 0 && Instant::now() < deadline {
        thread::yield_now();
    }
    let released = weak.strong_count() == 0;
    let subsequent = Runtime::new().unwrap();
    let shutdown = subsequent.shutdown_until(Instant::now() + Duration::from_secs(1));
    // Release the adversarial destructor before any assertions or ordinary Runtime drops.
    let _ = release.send(());
    caller.join().unwrap();
    dropped_from_managed.unwrap();
    subsequent.shutdown().unwrap();
    assert!(
        caller_owns_drop,
        "Shared retained the caller's blocking destructor"
    );
    assert!(
        released,
        "the lifecycle owner retained the custom error object"
    );
    assert!(matches!(shutdown, Ok(crate::ShutdownOutcome::Complete(_))));
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn generic_application_error_is_not_formatted_or_retained() {
    let runtime = Runtime::new().unwrap();
    let source = Arc::new(vec![0; 8 * 1024 * 1024]);
    let weak = Arc::downgrade(&source);
    let error = ScopeFailure::default()
        .finish_generic::<(), _>(Err(source), Ok(()), &runtime.shared)
        .unwrap_err();
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
    assert!(weak.upgrade().is_none());
    assert!(format!("{snapshot:?}").len() < 8192);
}
