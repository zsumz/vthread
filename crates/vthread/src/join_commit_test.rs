use crate::{
    Error, Result, Runtime, ScopeOptions, SuspensionReason, TaskStatus, context, local_scope,
    park_pair, signal::lock, support_test::until, task::SharedTaskRecord,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn policy_error(result: Result<()>, deadline: Option<Instant>) {
    assert!(
        matches!(
            (result.as_ref().map_err(Error::primary), deadline),
            (Err(Error::Cancelled), None) | (Err(Error::DeadlineExceeded), Some(_))
        ),
        "{result:?}"
    );
}

fn after_completion(record: &SharedTaskRecord, deadline: Option<Instant>) -> Arc<AtomicBool> {
    let mounted = context::current().unwrap();
    let execution = mounted.execution().unwrap();
    let parent = Arc::clone(&execution.record);
    let cancellation = execution.data.options.cancellation.clone();
    let child = lock(record).id;
    let checked = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&checked);
    *lock(&lock(record).completion.after_notify) = Some(Box::new(move |selected| {
        assert_eq!(
            selected, 1,
            "completion must select the parked join generation"
        );
        assert_eq!(
            lock(&parent).status,
            TaskStatus::Suspended(SuspensionReason::Join(child)),
            "the single carrier has not resumed the joiner"
        );
        if let Some(deadline) = deadline {
            assert!(
                Instant::now() < deadline,
                "completion must win before expiry"
            );
            until(|| Instant::now() >= deadline);
        } else {
            cancellation.cancel();
        }
        observed.store(true, Ordering::Release);
    }));
    checked
}

fn completion_first(local: bool, expires: bool) {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let deadline = expires.then(|| Instant::now() + Duration::from_secs(1));
    let options = deadline.map_or(ScopeOptions::default(), |d| {
        ScopeOptions::default().deadline(d)
    });
    let result = runtime.run_scope_with(options, |scope| {
        let mut parent = if local {
            scope.spawn("local completion winner", move || {
                let result = local_scope(|local| {
                    let mut child = local.spawn("local child", || 42)?;
                    assert!(!child.is_finished());
                    let checked = after_completion(&child.record, deadline);
                    let waited = child.wait();
                    assert!(checked.load(Ordering::Acquire));
                    assert!(waited.is_ok(), "completion selected first: {waited:?}");
                    child.wait().unwrap();
                    assert_eq!(child.join().unwrap(), 42);
                    child.wait().unwrap();
                    policy_error(crate::checkpoint(), deadline);
                    Ok(())
                });
                policy_error(result, deadline);
            })?
        } else {
            let (gate, release) = park_pair();
            let mut child = scope.spawn("transferable child", move || {
                gate.park().unwrap();
                42
            })?;
            scope.spawn("transferable completion winner", move || {
                assert!(!child.is_finished());
                let checked = after_completion(&child.record, deadline);
                release.unpark();
                let waited = child.wait();
                assert!(checked.load(Ordering::Acquire));
                assert!(waited.is_ok(), "completion selected first: {waited:?}");
                child.wait().unwrap();
                assert_eq!(child.join().unwrap(), 42);
                child.wait().unwrap();
                policy_error(crate::checkpoint(), deadline);
            })?
        };
        parent.join()?;
        Ok(())
    });
    if expires {
        policy_error(result, deadline);
    } else {
        result.unwrap();
    }
    runtime.shutdown().unwrap();
}

#[test]
fn transferable_completion_selected_before_cancellation_commits() {
    completion_first(false, false);
}

#[test]
fn local_completion_selected_before_cancellation_commits() {
    completion_first(true, false);
}

#[test]
fn transferable_completion_selected_before_inherited_deadline_commits() {
    completion_first(false, true);
}

#[test]
fn local_completion_selected_before_inherited_deadline_commits() {
    completion_first(true, true);
}

fn policy_first(local: bool, expires: bool) {
    let runtime = Runtime::builder().carriers(1).build().unwrap();
    let deadline = expires.then(|| Instant::now() + Duration::from_secs(1));
    let options = deadline.map_or(ScopeOptions::default(), |d| {
        ScopeOptions::default().deadline(d)
    });
    let result = runtime.run_scope_with(options, |scope| {
        let (gate, release_child) = park_pair();
        let (send_token, token) = mpsc::sync_channel(1);
        let mut parent = if local {
            scope.spawn("local policy winner", move || {
                let result = local_scope(|local| {
                    let mut child = local.spawn("pending local", || {
                        let _ = gate.park();
                        42
                    })?;
                    send_token.send(crate::cancellation_token()?).unwrap();
                    policy_error(child.wait(), deadline);
                    Ok(())
                });
                policy_error(result, deadline);
            })?
        } else {
            let mut child = scope.spawn("pending transferable", move || {
                let _ = gate.park();
                42
            })?;
            scope.spawn("transferable policy winner", move || {
                send_token
                    .send(crate::cancellation_token().unwrap())
                    .unwrap();
                policy_error(child.wait(), deadline);
            })?
        };
        let cancellation = token.recv_timeout(Duration::from_secs(5)).unwrap();
        until(|| runtime.snapshot().parked == 2);
        let (hold, release) = mpsc::sync_channel(1);
        let (entered, blocked) = mpsc::sync_channel(1);
        let mut barrier = scope.spawn("hold policy selection", move || {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        })?;
        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
        if let Some(deadline) = deadline {
            until(|| Instant::now() >= deadline);
        } else {
            cancellation.cancel();
        }
        hold.send(()).unwrap();
        // The child can finish before the selected joiner resumes. Its completion
        // must not overwrite a generation that policy already won.
        if !expires {
            release_child.unpark();
        }
        parent.join()?;
        barrier.join()?;
        Ok(())
    });
    if expires {
        policy_error(result, deadline);
    } else {
        result.unwrap();
    }
    runtime.shutdown().unwrap();
}

#[test]
fn cancellation_selecting_before_completion_interrupts_both_handle_types() {
    for local in [false, true] {
        policy_first(local, false);
    }
}

#[test]
fn inherited_deadline_selecting_before_completion_interrupts_both_handle_types() {
    for local in [false, true] {
        policy_first(local, true);
    }
}
