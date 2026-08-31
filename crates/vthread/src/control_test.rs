use super::Shared;
use crate::{Error, RuntimeConfig};

#[test]
fn scope_admission_is_exclusive_and_shutdown_is_terminal() {
    let shared = Shared::new(RuntimeConfig::default());
    let scope = shared.begin_scope().expect("scope");
    assert!(matches!(shared.begin_scope(), Err(Error::RootScopeActive)));
    shared.finish_scope(scope);
    assert!(shared.begin_scope().is_ok());
    shared.request_stop();
    assert!(matches!(
        shared.submit(scope, "late".into(), || ()),
        Err(Error::RuntimeStopped)
    ));
    assert!(matches!(shared.begin_scope(), Err(Error::RuntimeStopped)));
}

#[test]
fn snapshot_observation_does_not_block_admission_or_completion() {
    use crate::signal::lock;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let existing = shared.reserve(scope, "existing".into(), None).unwrap();
    let (observing, observed) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    *lock(&shared.snapshot_observe_hook) = Some(Box::new(move || {
        observing.send(()).unwrap();
        resumed.recv().unwrap();
    }));
    let (done, completed) = mpsc::channel();
    let progress = std::thread::scope(|threads| {
        let snapshot = threads.spawn(|| shared.snapshot());
        observed.recv_timeout(Duration::from_secs(5)).unwrap();
        threads.spawn(|| {
            let new = shared.reserve(scope, "new".into(), None).unwrap();
            shared.complete(&existing, None);
            shared.complete(&new, None);
            done.send(()).unwrap();
        });
        let progress = completed.recv_timeout(Duration::from_secs(2));
        resume.send(()).unwrap();
        snapshot.join().unwrap();
        progress
    });
    assert!(
        progress.is_ok(),
        "snapshot observation held admission/completion lock"
    );
    assert_eq!(shared.snapshot().active, 0);
    shared.finish_scope(scope);
}
