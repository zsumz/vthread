use crate::{
    Error, ScopeOptions, TaskStatus,
    options::TaskOptions,
    task::{SharedTaskRecord, TaskCell, TaskRecord},
    wait::WaitCell,
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

fn task(capacity: usize) -> SharedTaskRecord {
    Arc::new(TaskCell::new(
        TaskRecord {
            id: crate::TaskId::new(1),
            scope: 1,
            parent: None,
            options: Some(TaskOptions::root(ScopeOptions::default(), capacity)),
            name: "completion owner".into(),
            carrier: crate::CarrierId(0),
            deadline: None,
            failure: None,
            status: TaskStatus::Queued,
            parks: 0,
            last_suspension: None,
            last_wake: None,
            outcome_observed: false,
            panic: None,
        },
        capacity,
    ))
}

#[test]
fn subscriptions_are_bounded_and_unregister_on_drop() {
    let task = task(1);
    let completion = task.completion();
    let first = task.subscribe_completion(&WaitCell::new()).unwrap();
    assert!(matches!(
        task.subscribe_completion(&WaitCell::new()),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Waiters,
            ..
        })
    ));
    drop(first);
    let _second = task.subscribe_completion(&WaitCell::new()).unwrap();
    completion.complete();
    assert!(completion.done());
    let _late = task.subscribe_completion(&WaitCell::new()).unwrap();
}

#[test]
fn subscription_retains_its_embedded_completion_owner() {
    let task = task(1);
    let retained = Arc::downgrade(&task);
    let subscription = task.subscribe_completion(&WaitCell::new()).unwrap();
    drop(task);
    assert!(retained.upgrade().is_some());
    drop(subscription);
    assert!(retained.upgrade().is_none());
}

#[test]
fn completion_without_subscribers_does_not_lock_the_waiter_map() {
    let task = task(1);
    let waiter_map = crate::signal::lock(&task.completion().state);
    let completing = Arc::clone(&task);
    let (sent, received) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        completing.completion().complete();
        sent.send(()).unwrap();
    });

    received
        .recv_timeout(Duration::from_secs(1))
        .expect("uncontended completion locked the empty waiter map");
    drop(waiter_map);
    worker.join().unwrap();
}
