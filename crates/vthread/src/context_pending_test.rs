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
                    let retained = execution
                        .take_wait(request.token())
                        .unwrap()
                        .expect("shared wait registration");
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
fn a_task_resident_wait_is_recognized_only_by_exact_identity() {
    let runtime = crate::Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("task-resident-wait", || {
                    let mounted = current().unwrap();
                    let execution = mounted.execution().unwrap();
                    let wait = execution.synchronization_wait().unwrap();
                    let WaitBegin::Park { request, .. } = wait
                        .begin(execution.id, execution.task_key(), execution.hub(), None)
                        .unwrap()
                    else {
                        panic!("expected a park request");
                    };
                    let token = request.token();
                    let wrong = vthread_stack::ParkToken::new(token.wait() + 1, token.generation());
                    assert!(execution.take_wait(wrong).is_err());
                    let stale = vthread_stack::ParkToken::new(token.wait(), token.generation() + 1);
                    assert!(execution.take_wait(stale).is_err());
                    assert!(execution.take_wait(token).unwrap().is_none());
                    wait.rollback(token);
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
