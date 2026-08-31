use crate::{Error, Runtime, Spawner};

#[test]
fn capability_is_transferable_and_does_not_extend_its_root() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Spawner>();
    let runtime = Runtime::new().unwrap();
    let spawner = runtime
        .run_scope(|scope| {
            let spawner = scope.spawner();
            assert_eq!(spawner.spawn("answer", || 42)?.join()?, 42);
            Ok(spawner)
        })
        .unwrap();
    assert!(matches!(
        spawner.spawn("closed", || ()),
        Err(Error::ScopeClosed)
    ));
    drop(runtime);
    assert!(matches!(
        spawner.spawn("gone", || ()),
        Err(Error::ScopeClosed)
    ));
}

#[test]
fn running_task_can_spawn_a_transferable_child_on_another_carrier() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    runtime
        .run_scope(|scope| {
            let spawner = scope.spawner();
            scope
                .spawn("parent", move || {
                    let owner = std::thread::current().id();
                    let mut child = spawner.spawn("child", move || {
                        assert_ne!(std::thread::current().id(), owner);
                        let owner = std::thread::current().id();
                        for _ in 0..8 {
                            crate::yield_now()?;
                            assert_eq!(std::thread::current().id(), owner);
                        }
                        Ok::<_, Error>(42)
                    })?;
                    child.join()?
                })?
                .join()??;
            let snapshot = runtime.snapshot();
            let parent = snapshot
                .tasks()
                .iter()
                .find(|task| task.name() == "parent")
                .unwrap();
            let child = snapshot
                .tasks()
                .iter()
                .find(|task| task.name() == "child")
                .unwrap();
            assert_eq!(child.parent(), Some(parent.id()));
            assert_eq!(child.scope(), parent.scope());
            Ok(())
        })
        .unwrap();
}
