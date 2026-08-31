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
fn an_interrupted_cross_runtime_join_retains_its_handle_and_result() {
    use crate::{Error, ScopeOptions};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let target_runtime = Runtime::new().unwrap();
    let owner = target_runtime.supervisor(ScopeOptions::default()).unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let mut target = owner
        .spawn("target", move || {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            42
        })
        .unwrap();
    assert!(matches!(target.take_result(), Err(Error::WouldBlock)));
    let runtime = Runtime::new().unwrap();
    let (returned, receive) = mpsc::sync_channel(1);
    let _ = runtime.run_scope_with(
        ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(20)),
        |scope| {
            scope
                .spawn("interruptible waiter", move || {
                    assert!(matches!(target.join(), Err(Error::DeadlineExceeded)));
                    returned.send(target).unwrap();
                })?
                .join()
        },
    );
    let mut target = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    target.wait().unwrap();
    assert_eq!(target.take_result().unwrap(), 42);
    assert!(matches!(target.join(), Err(Error::ResultAlreadyTaken)));
    owner.shutdown().unwrap();
}
