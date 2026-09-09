#[path = "admission_progress_test.rs"]
mod admission_progress_test;
use crate::control::Shared;
pub(crate) use admission_progress_test::{
    TestAdmissionPhase, TestAdmissionProgress, install_admission_progress, record_admission_phase,
    record_admission_rejection,
};
use std::{
    io::{Read, Write},
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) struct IsolatedOutput {
    pub(crate) status: ExitStatus,
    pub(crate) timed_out: bool,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) fn run_isolated(test: &str, child: (&str, &str), timeout: Duration) -> IsolatedOutput {
    let mut process = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(child.0, child.1)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn isolated test");
    let stdout = drain(process.stdout.take().expect("child stdout"));
    let stderr = drain(process.stderr.take().expect("child stderr"));
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = process.try_wait().expect("poll isolated test") {
            break (status, false);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let status = match process.kill() {
                Ok(()) => process.wait().expect("reap timed out test"),
                Err(error) => process
                    .try_wait()
                    .expect("poll after failed kill")
                    .unwrap_or_else(|| panic!("kill timed out test: {error}")),
            };
            break (status, true);
        }
        thread::park_timeout(remaining.min(Duration::from_millis(10)));
    };
    IsolatedOutput {
        status,
        timed_out,
        stdout: stdout.join().expect("stdout reader"),
        stderr: stderr.join().expect("stderr reader"),
    }
}

fn drain(mut stream: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        stream.read_to_end(&mut output).expect("read child output");
        output
    })
}

pub(crate) fn wait_without_intervention(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

#[derive(Default)]
pub(crate) struct RefillCounters {
    pub(crate) accepted: AtomicUsize,
    pub(crate) started: AtomicUsize,
    pub(crate) returned: AtomicUsize,
    pub(crate) cleanup: AtomicBool,
    pub(crate) admission: Arc<TestAdmissionProgress>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RefillBeforeStop {
    pub(crate) accepted_begin: usize,
    pub(crate) accepted_end: usize,
    pub(crate) queued: usize,
    pub(crate) started: usize,
    pub(crate) body_returns: usize,
    pub(crate) completed_credits: u64,
    pub(crate) active: usize,
}

pub(crate) fn observe_refill_passive(shared: &Shared, counters: &RefillCounters, phase: &str) {
    let accepted = counters.accepted.load(Ordering::SeqCst);
    let queued = shared.inboxes[0].pending();
    let started = counters.started.load(Ordering::SeqCst);
    let body_returns = counters.returned.load(Ordering::SeqCst);
    let epoch = shared.inboxes[0].signal.version();
    let waiting = shared.inboxes[0].signal.waiting();
    let cleanup = counters.cleanup.load(Ordering::SeqCst);
    let carrier = shared.inboxes[0].signal.test_progress.snapshot();
    let producer = counters.admission.snapshot();
    assert!(!cleanup, "progress evidence captured after cleanup");
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "refill-lock-free phase={phase} accepted={accepted} queued={queued} started={started} \
         body_returns={body_returns} epoch={epoch} waiting={waiting} cleanup={cleanup} \
         carrier={carrier:?} producer={producer:?}",
    )
    .unwrap();
    output.flush().unwrap();
}

pub(crate) fn observe_refill_rich(
    shared: &Shared,
    scope: u64,
    counters: &RefillCounters,
) -> RefillBeforeStop {
    assert!(
        !counters.cleanup.load(Ordering::SeqCst),
        "progress evidence captured after cleanup"
    );
    let accepted_begin = counters.accepted.load(Ordering::SeqCst);
    let snapshot = shared.snapshot();
    let report = shared.scope_report(scope);
    let before = RefillBeforeStop {
        accepted_begin,
        accepted_end: counters.accepted.load(Ordering::SeqCst),
        queued: shared.inboxes[0].pending(),
        started: counters.started.load(Ordering::SeqCst),
        body_returns: counters.returned.load(Ordering::SeqCst),
        completed_credits: report.completed,
        active: snapshot.active,
    };
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "refill-before-stop progress={before:?} accepting={} epoch={} waiting={} \
         scope={report:?} carriers={:?}",
        snapshot.accepting,
        shared.inboxes[0].signal.version(),
        shared.inboxes[0].signal.waiting(),
        snapshot.carriers
    )
    .unwrap();
    output.flush().unwrap();
    before
}

#[test]
fn isolated_supervisor_bounds_stalled_child_and_preserves_output() {
    const CHILD: &str = "VTHREAD_STALLED_CHILD";
    if std::env::var(CHILD).as_deref() == Ok("1") {
        let payload = vec![b'x'; 128 * 1_024];
        {
            let mut output = std::io::stdout().lock();
            output.write_all(&payload).unwrap();
            writeln!(output, "stdout-tail").unwrap();
            output.flush().unwrap();
        }
        {
            let mut error = std::io::stderr().lock();
            error.write_all(&payload).unwrap();
            writeln!(error, "stderr-tail").unwrap();
            error.flush().unwrap();
        }
        loop {
            thread::park_timeout(Duration::from_secs(1));
        }
    }
    let output = run_isolated(
        "support_test::isolated_supervisor_bounds_stalled_child_and_preserves_output",
        (CHILD, "1"),
        Duration::from_secs(2),
    );
    assert!(output.timed_out);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("stdout-tail"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("stderr-tail"));
}

pub(crate) fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for test synchronization"
        );
        thread::yield_now();
    }
}
