use crate::{Error, Runtime, SuspensionReason, TaskStatus, park_pair};
use std::{fmt, time::Duration};

#[test]
fn stall_evidence_survives_reclamation_and_is_replaced_by_the_next_stall() {
    let runtime = Runtime::builder()
        .max_vthreads(2)
        .stack_cache_capacity(2)
        .stall_policy(crate::StallPolicy::AbortAfter(Duration::from_millis(5)))
        .build()
        .unwrap();
    for name in ["first\nforged-row", "second"] {
        let (park, _wake) = park_pair();
        let result = runtime.run_scope(|scope| {
            let _ = scope.spawn(name, move || park.park())?;
            Ok(())
        });
        assert!(matches!(
            result.as_ref().map_err(Error::primary),
            Err(Error::RuntimeStalled { active: 1 })
        ));
        let snapshot = runtime.snapshot();
        assert!(snapshot.tasks.is_empty());
        let stalled = snapshot.last_stall.as_ref().unwrap();
        assert_eq!(stalled.tasks.len(), 1);
        assert_eq!(stalled.tasks[0].name, name);
        assert_eq!(
            stalled.tasks[0].status,
            TaskStatus::Suspended(SuspensionReason::Park)
        );
        assert!(stalled.quiescent_for >= Duration::from_millis(5));
        let mut text = String::new();
        snapshot.write_dump(&mut text).unwrap();
        assert!(text.contains("stalled_task"));
        assert!(!text.contains("\nforged-row"));
        runtime
            .run_scope(|scope| scope.spawn("healthy", || ())?.join())
            .unwrap();
        assert_eq!(runtime.snapshot().last_stall.unwrap(), *stalled);
    }
}

#[test]
fn destination_failure_stops_the_dump() {
    struct Full;
    impl fmt::Write for Full {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    assert!(
        crate::RuntimeSnapshot::empty(crate::identity::RuntimeId::next())
            .write_dump(&mut Full)
            .is_err()
    );
}

#[test]
fn aggregate_dump_reports_truncation_and_runtime_identity() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("long metadata ".repeat(8), || ())?;
            task.wait()?;
            let mut snapshot = scope.runtime_snapshot();
            snapshot.tasks = vec![snapshot.tasks[0].clone(); 1000];
            let mut output = String::new();
            let report = snapshot.write_dump(&mut output).unwrap();
            assert!(report.truncated());
            assert!(report.bytes() <= 64 * 1024);
            assert_eq!(report.bytes(), output.len());
            assert!(output.ends_with("[dump truncated]\n"));
            assert!(output.contains(&format!("runtime={}", runtime.id())));
            task.join()?;
            Ok(())
        })
        .unwrap();
}
