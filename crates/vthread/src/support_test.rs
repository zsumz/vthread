#[path = "admission_progress_test.rs"]
mod admission_progress_test;
pub(crate) use admission_progress_test::{
    TestAdmissionPhase, TestAdmissionProgress, install_admission_progress, record_admission_phase,
    record_admission_rejection,
};
use std::{
    io::{Read, Write},
    process::{Command, ExitStatus, Stdio},
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
