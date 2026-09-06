use super::run;
use crate::{CarrierId, CarrierStatus, RuntimeConfig, TaskFailure, control::Shared};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[test]
fn scheduler_fault_reclaims_the_mounted_stack_and_marks_carrier_failed() {
    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = DropFlag(Arc::clone(&dropped));
    shared
        .submit(scope, "fault".into(), move || {
            let _flag = flag;
            crate::yield_now().expect("yield");
            panic!("must not resume after scheduler failure");
        })
        .expect("submit");
    shared.fail_after_resume.store(true, Ordering::SeqCst);
    let worker_shared = Arc::clone(&shared);
    let worker = thread::spawn(move || run(worker_shared, CarrierId(0)));
    crate::support_test::until(|| shared.snapshot().carriers[0].status == CarrierStatus::Failed);
    worker.join().expect("carrier fault was contained");
    assert!(dropped.load(Ordering::SeqCst));
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.stats.aborted, 1);
    assert_eq!(snapshot.tasks[0].failure, Some(TaskFailure::CarrierFailed));
    shared.finish_scope(scope);
    assert!(matches!(
        shared.begin_scope(),
        Err(crate::Error::RuntimeStopped)
    ));
}
