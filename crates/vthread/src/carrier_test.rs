use super::run;
use crate::{CarrierId, CarrierStatus, RuntimeConfig, TaskFailure, control::Shared};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[test]
fn continuously_refilled_coalesced_inbox_is_fully_drained() {
    const TASKS: usize = 4_096;

    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let worker_shared = Arc::clone(&shared);
    let worker = thread::spawn(move || run(worker_shared, CarrierId(0)));
    let submitted_shared = Arc::clone(&shared);
    let (submitted, submitted_rx) = mpsc::sync_channel(1);
    let (completed, completed_rx) = mpsc::sync_channel(1);
    let producer = thread::spawn(move || {
        for index in 0..TASKS {
            loop {
                let completed = completed.clone();
                let submission =
                    submitted_shared.submit(scope, format!("task-{index}"), move || {
                        if index + 1 == TASKS {
                            completed.send(()).expect("completion observer");
                        }
                    });
                match submission {
                    Ok(_) => break,
                    Err(crate::Error::Capacity {
                        resource: crate::error::CapacityResource::CarrierQueue,
                        ..
                    }) => thread::yield_now(),
                    Err(error) => panic!("unexpected admission error: {error}"),
                }
            }
        }
        submitted.send(()).expect("submission observer");
    });

    let admission = submitted_rx.recv_timeout(Duration::from_secs(5));
    let completion = admission
        .as_ref()
        .map(|_| completed_rx.recv_timeout(Duration::from_secs(5)));
    if admission.is_ok() && completion.as_ref().is_ok_and(|result| result.is_ok()) {
        shared.wait(scope, None).expect("drain admitted tasks");
    }
    shared.request_stop();
    let producer = producer.join();
    let worker = worker.join();
    shared.finish_scope(scope);
    admission.expect("continuous refill stalled admission");
    completion
        .expect("completion was not observed")
        .expect("continuous refill stranded accepted tasks");
    producer.expect("producer");
    worker.expect("carrier");
}

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
