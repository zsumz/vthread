use crate::Runtime;

#[test]
fn joining_returns_the_typed_result() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .scope(|scope| {
            let handle = scope.spawn("answer", || 42_u64)?;
            assert_eq!(handle.task_id().to_string(), "1");
            assert_eq!(handle.join()?, 42);
            Ok(())
        })
        .expect("scope succeeds");
}
