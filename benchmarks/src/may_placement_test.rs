use super::{PairProbe, TaskTrace, summarize};
use std::sync::Arc;

#[test]
fn stable_tasks_report_one_owner_without_migration() {
    let probe = Arc::new(PairProbe::new());
    probe.record(0, TaskTrace::start());
    probe.record(1, TaskTrace::start());

    let (owners, migrations) = summarize(&[probe]);

    assert_eq!(owners, vec![(0, 0)]);
    assert_eq!(migrations, vec![false, false]);
}

#[test]
fn movement_between_threads_is_reported() {
    let probe = Arc::new(PairProbe::new());
    let trace = TaskTrace::start();
    let moved = std::thread::spawn(move || {
        let mut trace = trace;
        trace.observe();
        trace
    })
    .join()
    .expect("placement observer thread panicked");
    probe.record(0, moved);
    probe.record(1, TaskTrace::start());

    let (_, migrations) = summarize(&[probe]);

    assert_eq!(migrations, vec![true, false]);
}
