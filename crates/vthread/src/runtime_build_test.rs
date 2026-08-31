use crate::{Error, Runtime, ThreadComponent, signal::lock};
use std::{cell::Cell, sync::atomic::Ordering};

thread_local! {
    static FAULT: Cell<(usize, u8)> = const { Cell::new((usize::MAX, 0)) };
}

pub(super) fn inject(runtime: &Runtime, stage: usize) -> crate::Result<()> {
    let (target, cleanup) = FAULT.get();
    if target != stage {
        return Ok(());
    }
    if cleanup == 1 {
        *lock(&runtime.shared.carrier_exit_hook) =
            Some(Box::new(|| panic!("partial construction cleanup failure")));
    } else if cleanup == 2 {
        runtime
            .shared
            .fail_coordinator_before_drain
            .store(true, Ordering::Release);
    }
    Err(Error::thread_start(
        if stage == 0 {
            ThreadComponent::Readiness
        } else {
            ThreadComponent::Carrier
        },
        std::io::Error::other("injected component construction failure"),
    ))
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        FAULT.set((usize::MAX, 0));
    }
}

fn build(stage: usize, cleanup: u8) -> Error {
    FAULT.set((stage, cleanup));
    let _reset = Reset;
    Runtime::builder().carriers(2).build().unwrap_err()
}

#[test]
fn partial_construction_preserves_both_start_and_cleanup_failures() {
    let error = build(2, 1);
    let detail = format!("{error:?}");
    assert!(
        detail.contains("ThreadStart"),
        "construction failure missing: {detail}"
    );
    assert!(
        detail.contains("ShutdownFailed"),
        "cleanup failure missing: {detail}"
    );
    let Error::ConstructionFailed(failure) = error else {
        panic!("missing paired failure")
    };
    assert!(matches!(
        failure.construction(),
        Error::ThreadStart {
            component: ThreadComponent::Carrier,
            ..
        }
    ));
    let Error::ShutdownFailed(report) = failure.shutdown() else {
        panic!("missing cleanup report")
    };
    assert!(
        report
            .failures()
            .entries()
            .iter()
            .any(|entry| entry.component() == ThreadComponent::Carrier)
    );
}

#[test]
fn successful_rollback_keeps_the_original_construction_error() {
    for stage in [0, 1, 2] {
        assert!(matches!(build(stage, 0), Error::ThreadStart { .. }));
        Runtime::new().unwrap().shutdown().unwrap();
    }
}

#[test]
fn generic_runner_preserves_construction_and_rollback_errors_without_running_body() {
    FAULT.set((2, 1));
    let _reset = Reset;
    // Fail the final initialization boundary after the default carrier starts.
    let result = crate::try_run(|_| -> Result<(), &'static str> {
        panic!("body ran after failed construction")
    });
    let failure = result.unwrap_err();
    assert!(failure.body().is_none());
    assert!(matches!(
        failure.scope(),
        Some(Error::ThreadStart {
            component: ThreadComponent::Carrier,
            ..
        })
    ));
    assert!(matches!(failure.shutdown(), Some(Error::ShutdownFailed(_))));
}

#[test]
fn partial_construction_retains_lifecycle_fail_stop_as_a_second_cause() {
    const CHILD: &str = "VTHREAD_BUILD_CLEANUP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "runtime::runtime_build::runtime_build_test::partial_construction_retains_lifecycle_fail_stop_as_a_second_cause", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    let Error::ConstructionFailed(failure) = build(2, 2) else {
        panic!("missing paired lifecycle failure")
    };
    assert!(matches!(failure.construction(), Error::ThreadStart { .. }));
    assert!(matches!(failure.shutdown(), Error::LifecycleFailed(_)));
    assert!(matches!(Runtime::new(), Err(Error::LifecycleFailed(_))));
}
