//! Establish WouldBlock, peer readiness and retry boundaries by owner dispatch.

use crate::{
    CarrierId, Runtime, SuspensionReason, TaskFailure, control::Shared, kernel::Kernel,
    services::Services,
};
use std::{
    io::{Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

struct Driver {
    shared: Arc<Shared>,
    scope: u64,
    kernel: Kernel,
}

impl Driver {
    fn new() -> Self {
        let config = Runtime::builder()
            .max_vthreads(4)
            .carrier_queue_capacity(4)
            .stack_cache_capacity(4)
            .io_capacity(1)
            .blocking_threads(1)
            .blocking_capacity(1)
            .build()
            .unwrap()
            .config();
        let shared = Arc::new(Shared::new(config));
        let services = Services::new(config, Arc::downgrade(&shared)).unwrap();
        assert!(shared.services.set(services).is_ok());
        let scope = shared.begin_scope().unwrap();
        let kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
        Self {
            shared,
            scope,
            kernel,
        }
    }

    fn waits(&self) -> usize {
        self.shared
            .services
            .get()
            .unwrap()
            .snapshot()
            .readiness_waits()
    }

    fn materialize(&mut self) {
        assert!(!self.kernel.receive());
        assert_eq!(
            self.kernel.ready.len(),
            2,
            "both peers must be runnable before the first mount"
        );
    }

    fn drain(&mut self, complete: &AtomicBool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let progressed = self.kernel.tick(true).unwrap();
            if complete.load(Ordering::Acquire) && !progressed {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ordered I/O did not complete after the explicit write"
            );
            std::thread::yield_now();
        }
        self.shared.wait(self.scope, None).unwrap();
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        // This is the ordinary test driver, not a mounted task. Keep the service
        // publisher alive until any in-flight selected wait can retire legally.
        while !self.kernel.abort(None, TaskFailure::RuntimeStopped) {
            std::thread::yield_now();
        }
        self.shared.finish_scope(self.scope);
    }
}

fn reader(
    driver: &Driver,
    socket: UnixStream,
    attempts: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
    prepare: impl FnOnce() + Send + 'static,
) {
    driver
        .shared
        .submit(driver.scope, "ordered reader".into(), move || {
            prepare();
            let mut byte = [0];
            let count = super::operation(
                socket.as_fd(),
                zio::Interest::READABLE,
                SuspensionReason::IoRead,
                || {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    (&socket).read(&mut byte)
                },
            )
            .unwrap();
            assert_eq!((count, byte), (1, *b"x"));
            done.store(true, Ordering::Release);
        })
        .unwrap();
}

fn sockets() -> (UnixStream, UnixStream) {
    let (reader, writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.set_nonblocking(true).unwrap();
    (reader, writer)
}

#[test]
fn blocked_io_yields_to_runnable_work_before_registering_readiness() {
    let mut driver = Driver::new();
    let (socket, mut writer) = sockets();
    let attempts = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    reader(
        &driver,
        socket,
        Arc::clone(&attempts),
        Arc::clone(&done),
        || {},
    );
    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    let sample = Arc::clone(&observed);
    let shared = Arc::clone(&driver.shared);
    driver
        .shared
        .submit(driver.scope, "ordered writer".into(), move || {
            sample.store(
                shared.services.get().unwrap().snapshot().readiness_waits(),
                Ordering::Release,
            );
            writer.write_all(b"x").unwrap();
        })
        .unwrap();
    driver.materialize();
    let _route = crate::context::mount_carrier(&driver.kernel.inbox.hub, &driver.kernel.local);
    assert!(driver.kernel.tick(false).unwrap());
    let first = (
        attempts.load(Ordering::Relaxed),
        driver.kernel.stats.yields,
        driver.kernel.stats.parks,
        driver.waits(),
    );
    assert!(driver.kernel.tick(false).unwrap()); // Only now may the peer write.
    driver.drain(&done);
    let final_counts = (
        attempts.load(Ordering::Relaxed),
        driver.kernel.stats.parks,
        driver.waits(),
    );
    drop(driver); // Preserve the first crossing before any negative-control cleanup.
    assert_eq!(
        first,
        (1, 1, 0, 0),
        "empty read with a materialized peer must yield before registering"
    );
    assert_eq!(
        observed.load(Ordering::Acquire),
        0,
        "writer ran before any readiness registration"
    );
    assert_eq!(final_counts, (2, 0, 0));
}

#[test]
fn blocked_io_registers_at_the_exact_retry_boundary_under_runnable_load() {
    let mut driver = Driver::new();
    let (socket, mut writer) = sockets();
    let attempts = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    reader(
        &driver,
        socket,
        Arc::clone(&attempts),
        Arc::clone(&done),
        || {},
    );
    let stop = Arc::clone(&done);
    driver
        .shared
        .submit(driver.scope, "runnable peer".into(), move || {
            while !stop.load(Ordering::Acquire) {
                crate::yield_now().unwrap();
            }
        })
        .unwrap();
    driver.materialize();
    let _route = crate::context::mount_carrier(&driver.kernel.inbox.hub, &driver.kernel.local);
    for _ in 0..super::RUNNABLE_YIELD_LIMIT {
        assert!(driver.kernel.tick(false).unwrap()); // Empty read / cooperative yield.
        assert!(driver.kernel.tick(false).unwrap()); // The already-runnable peer.
    }
    let before = (
        attempts.load(Ordering::Relaxed),
        driver.kernel.stats.parks,
        driver.waits(),
    );
    assert!(driver.kernel.tick(false).unwrap());
    let boundary = (
        attempts.load(Ordering::Relaxed),
        driver.kernel.stats.parks,
        driver.waits(),
    );
    writer.write_all(b"x").unwrap(); // Make the selected registration eligible only now.
    driver.drain(&done);
    let after = (attempts.load(Ordering::Relaxed), driver.waits());
    drop(driver);
    assert_eq!(before, (super::RUNNABLE_YIELD_LIMIT, 0, 0));
    assert_eq!(
        boundary,
        (super::RUNNABLE_YIELD_LIMIT + 1, 1, 1),
        "retry exhaustion must register exactly once"
    );
    assert_eq!(after, (super::RUNNABLE_YIELD_LIMIT + 2, 0));
}

#[test]
fn admission_after_selection_can_legally_register_before_the_peer_runs() {
    let mut driver = Driver::new();
    let (socket, mut writer) = sockets();
    let attempts = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    let sample = Arc::clone(&observed);
    let shared = Arc::clone(&driver.shared);
    let scope = driver.scope;
    reader(
        &driver,
        socket,
        Arc::clone(&attempts),
        Arc::clone(&done),
        move || {
            // Reproduce admission between scheduler selection and the empty read.
            // The cached runnable hint was sampled before this new arrival.
            assert!(!crate::context::carrier_has_runnable());
            let services = Arc::clone(&shared);
            shared
                .submit(scope, "late writer".into(), move || {
                    sample.store(
                        services
                            .services
                            .get()
                            .unwrap()
                            .snapshot()
                            .readiness_waits(),
                        Ordering::Release,
                    );
                    writer.write_all(b"x").unwrap();
                })
                .unwrap();
            assert!(!crate::context::carrier_has_runnable());
        },
    );
    assert!(!driver.kernel.receive());
    assert_eq!(driver.kernel.ready.len(), 1);
    let _route = crate::context::mount_carrier(&driver.kernel.inbox.hub, &driver.kernel.local);
    assert!(driver.kernel.tick(false).unwrap());
    let first = (
        attempts.load(Ordering::Relaxed),
        driver.kernel.stats.parks,
        driver.waits(),
    );
    assert!(!driver.kernel.receive());
    assert_eq!(driver.kernel.ready.len(), 1);
    assert!(driver.kernel.tick(false).unwrap());
    driver.drain(&done);
    let final_waits = driver.waits();
    drop(driver);
    assert_eq!(first, (1, 1, 1));
    assert_eq!(observed.load(Ordering::Acquire), 1);
    assert_eq!(final_waits, 0);
}
