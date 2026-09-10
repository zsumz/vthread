//! Production-shaped admission liveness regressions with external cleanup watchdogs.

use std::{
    io::{Read, Write},
    panic::{AssertUnwindSafe, catch_unwind},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use vthread::{Error, Runtime, error::CapacityResource};
use vthread_stack::{FiberState, MappedStack, fiber_scope};

const TASKS: usize = 4_096;
const WATCHDOG: Duration = Duration::from_secs(5);
const REFILL_CHILD: &str = "VTHREAD_PRODUCTION_REFILL_CHILD";
const PANIC_CHILD: &str = "VTHREAD_LOCAL_ADMISSION_PANIC_CHILD";

#[test]
fn production_shaped_coalesced_inbox_refill_completes() {
    if std::env::var(REFILL_CHILD).as_deref() != Ok("1") {
        supervise(
            "production_shaped_coalesced_inbox_refill_completes",
            REFILL_CHILD,
            Duration::from_secs(25),
        );
        return;
    }
    let runtime = Runtime::builder()
        .carriers(1)
        .max_vthreads(TASKS)
        .carrier_queue_capacity(256)
        .build()
        .unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let returned = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    runtime
        .run_scope(|scope| {
            let spawner = scope.spawner();
            let producer_accepted = Arc::clone(&accepted);
            let returned = Arc::clone(&returned);
            let producer_retries = Arc::clone(&retries);
            thread::scope(|threads| {
                let (submitted, submitted_rx) = mpsc::sync_channel(1);
                let producer_returned = Arc::clone(&returned);
                let producer = threads.spawn(move || {
                    let mut handles = Vec::with_capacity(TASKS);
                    for index in 0..TASKS {
                        loop {
                            let returned = Arc::clone(&producer_returned);
                            match spawner.spawn(format!("refill-{index}"), move || {
                                returned.fetch_add(1, Ordering::Release);
                            }) {
                                Ok(handle) => {
                                    handles.push(handle);
                                    producer_accepted.fetch_add(1, Ordering::Release);
                                    break;
                                }
                                Err(Error::Capacity {
                                    resource: CapacityResource::CarrierQueue,
                                    ..
                                }) => {
                                    producer_retries.fetch_add(1, Ordering::Relaxed);
                                    thread::yield_now();
                                }
                                Err(error) => panic!("refill admission failed: {error}"),
                            }
                        }
                    }
                    let _ = submitted.send(handles);
                });
                let admission = submitted_rx.recv_timeout(WATCHDOG);
                if admission.is_err() {
                    record_progress("admission-deadline", &accepted, &returned, &retries);
                    wait_without_intervention(Duration::from_secs(1));
                    record_progress("admission-no-intervention", &accepted, &returned, &retries);
                }
                let mut handles = admission.expect("refill admission deadline");
                let deadline = Instant::now() + WATCHDOG;
                while returned.load(Ordering::Acquire) != TASKS && Instant::now() < deadline {
                    thread::yield_now();
                }
                let completed_on_time = returned.load(Ordering::Acquire) == TASKS;
                record_progress("completion-deadline", &accepted, &returned, &retries);
                if !completed_on_time {
                    wait_without_intervention(Duration::from_secs(1));
                    record_progress("completion-no-intervention", &accepted, &returned, &retries);
                }
                assert!(completed_on_time, "refill body completion deadline");
                for handle in &mut handles {
                    handle.join().expect("refill task");
                }
                producer.join().expect("refill producer");
            });
            Ok(())
        })
        .unwrap();
    assert_eq!(returned.load(Ordering::Acquire), TASKS);
    runtime.shutdown().unwrap();
}

#[test]
fn local_admission_panic_is_rolled_back() {
    const RUNTIME_STACK: usize = 64 * 1024;
    if std::env::var(PANIC_CHILD).as_deref() != Ok("1") {
        supervise(
            "local_admission_panic_is_rolled_back",
            PANIC_CHILD,
            Duration::from_secs(30),
        );
        return;
    }
    let runtime = Runtime::builder()
        .carriers(1)
        .stack_size(RUNTIME_STACK)
        .max_vthreads(2)
        .carrier_queue_capacity(2)
        .stack_cache_capacity(0)
        .build()
        .unwrap();
    runtime
        .run_scope(|root| {
            root.spawn("parent", || {
                vthread::local_scope(|local| {
                    fiber_scope(1, |helpers| {
                        let stack = MappedStack::new(16 * 1024 * 1024, 0).unwrap();
                        let helper = helpers
                            .spawn(stack, || {
                                let capture = [7u8; 128 * 1024];
                                let entry = move || std::hint::black_box(capture);
                                assert!(std::mem::size_of_val(&entry) > RUNTIME_STACK);
                                match catch_unwind(AssertUnwindSafe(|| {
                                    local.spawn("oversized-entry", entry)
                                })) {
                                    Ok(Err(_)) => {}
                                    Err(payload) => {
                                        let message = payload
                                            .downcast_ref::<String>()
                                            .map(String::as_str)
                                            .or_else(|| payload.downcast_ref::<&str>().copied())
                                            .unwrap_or("non-string panic");
                                        assert!(message.contains("fiber entry does not fit"));
                                    }
                                    Ok(Ok(_)) => panic!("oversized entry was accepted"),
                                }
                            })
                            .unwrap();
                        assert!(matches!(helper.resume(), Some(FiberState::Complete)));
                    });
                    assert_eq!(local.spawn("after-rejected-entry", || 52)?.join()?, 52);
                    Ok(())
                })
                .unwrap();
            })?
            .join()?;
            Ok(())
        })
        .unwrap();
    let snapshot = runtime.snapshot();
    assert_eq!((snapshot.active(), snapshot.stats().admitted()), (0, 2));
    assert_eq!(snapshot.stats().rejected(), 1);
    runtime.shutdown().unwrap();
}

fn record_progress(
    phase: &str,
    accepted: &AtomicUsize,
    returned: &AtomicUsize,
    retries: &AtomicUsize,
) {
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "production-refill phase={phase} accepted={} returned={} retries={}",
        accepted.load(Ordering::Acquire),
        returned.load(Ordering::Acquire),
        retries.load(Ordering::Acquire),
    )
    .unwrap();
    output.flush().unwrap();
}

fn wait_without_intervention(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

fn supervise(name: &str, child_flag: &str, timeout: Duration) {
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(child_flag, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn production-shaped refill");
    let stdout = drain(child.stdout.take().expect("child stdout"));
    let stderr = drain(child.stderr.take().expect("child stderr"));
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().expect("poll refill child") {
            break (status, false);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let status = match child.kill() {
                Ok(()) => child.wait().expect("reap refill child"),
                Err(error) => child
                    .try_wait()
                    .expect("poll after failed kill")
                    .unwrap_or_else(|| panic!("kill refill child: {error}")),
            };
            break (status, true);
        }
        thread::park_timeout(remaining.min(Duration::from_millis(10)));
    };
    let stdout = stdout.join().expect("stdout reader");
    let stderr = stderr.join().expect("stderr reader");
    assert!(
        !timed_out && status.success() && String::from_utf8_lossy(&stdout).contains("1 passed"),
        "isolated admission test failed: status={status:?} timed_out={timed_out}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr),
    );
}

fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output).expect("read child output");
        output
    })
}
