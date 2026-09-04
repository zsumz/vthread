use super::super::current;
use crate::wait::{WaitBegin, WaitCell};

#[test]
fn a_published_wait_is_taken_once_by_exact_generation() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("published-wait", || {
                    let mounted = current().unwrap();
                    let execution = mounted.execution().unwrap();
                    let wait = WaitCell::new();
                    let WaitBegin::Park {
                        request,
                        registration,
                    } = wait
                        .begin(execution.id, execution.task_key(), execution.hub(), None)
                        .unwrap()
                    else {
                        panic!("expected a park request");
                    };
                    let expected = registration.state.clone();
                    let publication = execution
                        .publish_wait(request.token(), registration)
                        .unwrap();
                    let retained = execution.take_wait(request.token()).unwrap();
                    assert!(std::sync::Weak::ptr_eq(&retained.state, &expected));
                    assert!(execution.take_wait(request.token()).is_err());
                    drop(publication);
                    wait.rollback(request.token());
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn dropping_a_publication_clears_the_pending_handoff() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("rolled-back-publication", || {
                    let mounted = current().unwrap();
                    let execution = mounted.execution().unwrap();
                    let wait = WaitCell::new();
                    let WaitBegin::Park {
                        request,
                        registration,
                    } = wait
                        .begin(execution.id, execution.task_key(), execution.hub(), None)
                        .unwrap()
                    else {
                        panic!("expected a park request");
                    };
                    let publication = execution
                        .publish_wait(request.token(), registration)
                        .unwrap();
                    drop(publication);
                    assert!(execution.take_wait(request.token()).is_err());
                    wait.rollback(request.token());
                })?
                .join()
        })
        .unwrap();
}
