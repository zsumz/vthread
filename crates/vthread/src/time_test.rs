use std::{
    cell::RefCell,
    rc::Rc,
    time::Duration,
};

use crate::{Runtime, sleep};

#[test]
fn sleeping_parks_instead_of_blocking_the_next_task() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Rc::new(RefCell::new(Vec::new()));

    runtime
        .scope(|scope| {
            let sleeper_trace = Rc::clone(&trace);
            let sleeper = scope.spawn("sleeper", move || {
                sleeper_trace.borrow_mut().push("sleep:start");
                sleep(Duration::from_millis(1)).expect("sleep task");
                sleeper_trace.borrow_mut().push("sleep:end");
            })?;
            let worker_trace = Rc::clone(&trace);
            let worker = scope.spawn("worker", move || {
                worker_trace.borrow_mut().push("worker");
            })?;

            sleeper.join()?;
            worker.join()?;
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(&*trace.borrow(), &["sleep:start", "worker", "sleep:end"]);
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.stats.parks, 1);
    assert_eq!(snapshot.stats.timeouts, 1);
}
