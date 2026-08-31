#[test]
fn retained_task_observation_is_immutable() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("child", || 42)?;
            task.wait()?;
            let snapshot = scope.runtime_snapshot();
            let child = snapshot
                .tasks()
                .iter()
                .find(|item| item.id() == task.task_id())
                .unwrap();
            assert!(!child.outcome_observed());
            assert_eq!(task.take_result()?, 42);
            assert!(!child.outcome_observed());
            Ok(())
        })
        .unwrap();
}
