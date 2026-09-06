use crate::Runtime;

#[test]
fn joining_returns_the_typed_result() {
    let runtime = Runtime::new().expect("build runtime");
    runtime
        .run_scope(|scope| {
            let mut handle = scope.spawn("answer", || 42_u64)?;
            assert_eq!(handle.task_id().to_string(), "1");
            assert_eq!(handle.join()?, 42);
            Ok(())
        })
        .expect("scope succeeds");
}

#[test]
fn completed_wait_remains_immediate_after_its_observed_record_is_evicted() {
    use std::{sync::mpsc, time::Duration};
    let runtime = Runtime::builder()
        .max_vthreads(2)
        .stack_cache_capacity(2)
        .build()
        .unwrap();
    let owner = runtime
        .supervisor_with(crate::ScopeOptions::default())
        .unwrap();
    let mut completed = owner.spawn("completed", || 42).unwrap();
    assert_eq!(completed.join().unwrap(), 42);
    let (park, wake) = crate::park_pair();
    let mut pending = owner.spawn("still active", move || park.park()).unwrap();
    crate::support_test::until(|| runtime.snapshot().parked == 1);
    owner
        .spawn("evict observed record", || ())
        .unwrap()
        .join()
        .unwrap();
    assert!(
        runtime
            .snapshot()
            .tasks
            .iter()
            .all(|task| task.id != completed.task_id())
    );

    let (sent, received) = mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || sent.send(completed.wait()).unwrap());
    let immediate = received.recv_timeout(Duration::from_secs(1));
    // Always release the old scope, including on the pre-fix regression path.
    wake.unpark();
    pending.join().unwrap().unwrap();
    waiter.join().unwrap();
    owner.shutdown().unwrap();
    assert!(
        matches!(immediate, Ok(Ok(()))),
        "finished handle waited for unrelated work"
    );
}
