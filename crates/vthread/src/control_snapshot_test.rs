use super::*;
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn high_cardinality_snapshots_allow_concurrent_admission_and_completion() {
    let config = crate::RuntimeConfig::default();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let records = (0..8_192)
        .map(|index| {
            shared
                .reserve(scope, format!("task-{index}"), None)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let barrier = Barrier::new(2);
    let observations = AtomicUsize::new(0);
    std::thread::scope(|threads| {
        threads.spawn(|| {
            barrier.wait();
            for _ in 0..16 {
                let snapshot = shared.snapshot();
                assert!(snapshot.tasks.len() >= 8_192);
                assert!(snapshot.tasks.len() <= 16_384);
                observations.fetch_add(snapshot.tasks.len(), Ordering::Relaxed);
            }
        });
        barrier.wait();
        for record in &records {
            let new = shared.reserve(scope, "replacement".into(), None).unwrap();
            shared.complete(record, None);
            shared.complete(&new, None);
            record.lock().outcome_observed = true;
            new.lock().outcome_observed = true;
        }
    });
    assert!(observations.load(Ordering::Relaxed) >= 16 * 8_192);
    assert_eq!(shared.snapshot().active, 0);
    shared.finish_scope(scope);
}
