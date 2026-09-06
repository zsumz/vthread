//! Hold an exact selected notice while completion loses the same join generation.

use super::kernel_policy_test::{Owner, wait_until};
use crate::{Error, JoinHandle, Result, SuspensionReason, wait::Publication};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
struct Observation {
    interrupted: Result<usize>,
    completed: Result<usize>,
    repeated: Result<usize>,
}

impl Observation {
    fn collect(mut join: impl FnMut() -> Result<usize>) -> Self {
        Self {
            interrupted: join(),
            completed: join(),
            repeated: join(),
        }
    }

    fn check(self) {
        assert!(
            matches!(self.interrupted, Err(Error::DeadlineExceeded)),
            "{self:?}"
        );
        assert_eq!(self.completed.unwrap(), 42);
        assert!(
            matches!(self.repeated, Err(Error::ResultAlreadyTaken)),
            "{:?}",
            self.repeated
        );
    }
}

#[derive(Debug)]
struct Report {
    observation: Option<Observation>,
    local_policy: Option<Result<()>>,
    checkpoint: Result<()>,
    borrowed: Option<usize>,
}

fn parent(owner: &Owner, local: bool) -> JoinHandle<Report> {
    let (gate, _release) = crate::park_pair();
    if local {
        owner.submit("borrowed deadline joiner", move || {
            let mut borrowed = 0;
            let mut observation = None;
            let local_policy = crate::local_scope(|scope| {
                let mut child = scope.spawn("borrowed pending child", || {
                    let _ = gate.park();
                    borrowed = 42;
                    borrowed
                })?;
                observation = Some(Observation::collect(|| child.join()));
                Ok(())
            });
            Report {
                observation,
                local_policy: Some(local_policy),
                checkpoint: crate::checkpoint(),
                borrowed: Some(borrowed),
            }
        })
    } else {
        let mut child = owner.submit("transferable pending child", move || {
            let _ = gate.park();
            42
        });
        owner.submit("transferable deadline joiner", move || Report {
            observation: Some(Observation::collect(|| child.join())),
            local_policy: None,
            checkpoint: crate::checkpoint(),
            borrowed: None,
        })
    }
}

#[test]
fn a_transferable_join_retains_its_selected_deadline_and_result_ownership() {
    policy_first(false);
}

#[test]
fn a_borrowed_join_retains_its_selected_deadline_and_result_ownership() {
    policy_first(true);
}

#[test]
fn an_expired_local_scope_can_disconnect_its_unused_observer_without_a_task_failure() {
    let mut owner = Owner::new(Some(Duration::from_secs(1)));
    let (sent, receive) = std::sync::mpsc::sync_channel(1);
    let mut parent = owner.submit("accepted before local policy expires", move || {
        crate::local_scope(|_| {
            sent.send(()).unwrap();
            Ok(())
        })
    });
    wait_until(owner.deadline.unwrap());
    assert!(!owner.kernel.receive());
    owner.drain();
    let result = parent.take_result();
    let observed = receive.try_recv(); // Parent completion orders sender destruction.
    writeln!(
        std::io::stdout().lock(),
        "deadline-join admission result={result:?} observer={observed:?} parks={}",
        owner.kernel.stats.parks
    )
    .unwrap();
    assert!(
        matches!(result, Ok(Err(Error::DeadlineExceeded))),
        "{result:?}"
    );
    assert!(matches!(
        observed,
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    assert_eq!(owner.kernel.stats.parks, 0);
    assert_eq!(owner.kernel.shared.snapshot().active, 0);
}

fn policy_first(local: bool) {
    let mut owner = Owner::new(Some(Duration::from_secs(1)));
    let deadline = owner.deadline.unwrap();
    let mut parent = parent(&owner, local);
    let parent_id = parent.task_id();
    for _ in 0..4 {
        assert!(!owner.kernel.receive());
        if owner.kernel.parked.len() == 2 {
            break;
        }
        if !owner.tick() {
            panic!(
                "join setup did not park both tasks: local={local} result={:?}",
                parent.take_result()
            );
        }
    }
    assert_eq!(owner.kernel.parked.len(), 2);
    let parent_park = owner
        .kernel
        .parked
        .iter()
        .find(|parked| owner.kernel.task(parked.task).execution().id == parent_id)
        .unwrap();
    let (parent_route, token) = (parent_park.task, parent_park.token);
    let registration = parent_park.registration.as_ref().unwrap().clone();
    let child_park = owner
        .kernel
        .parked
        .iter()
        .find(|parked| parked.task != parent_route)
        .unwrap();
    let child_route = child_park.task;
    assert_eq!(child_route.is_borrowed(), local);
    let child = Arc::clone(owner.kernel.task(child_route).execution().record());
    let child_id = child.lock().id;
    assert_eq!(
        owner.kernel.task(parent_route).execution().data.reason(),
        SuspensionReason::Join(child_id)
    );
    let completion_selections = Arc::new(AtomicUsize::new(usize::MAX));
    let observed = Arc::clone(&completion_selections);
    *crate::signal::lock(&child.completion().after_notify) = Some(Box::new(move |selected| {
        observed.store(selected, Ordering::Release);
    }));
    assert_eq!(owner.kernel.timers.active_count(), 2);
    wait_until(deadline);
    owner.kernel.expire_timers().unwrap();
    assert_eq!(registration.publication(token), Publication::Published);
    assert!(!registration.select_ready(token));
    assert!(!child.completion().done());
    assert!(!parent.is_finished());
    assert_eq!(owner.kernel.timers.active_count(), 0);
    assert!(owner.kernel.ready.is_empty());
    assert_eq!(owner.kernel.local.pending_wakes(), 0);

    // Delay only the already-selected parent's exact notice. Both records remain
    // owner-held; no ready-policy rewrite, timer rescheduling or generation reuse.
    let first = owner.kernel.inbox.hub.pop_wake().unwrap();
    let second = owner.kernel.inbox.hub.pop_wake().unwrap();
    assert!(owner.kernel.inbox.hub.pop_wake().is_none());
    let (held, child_notice) = if first.task == parent_id {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!((held.route, held.token), (parent_route, token));
    assert_eq!(
        (child_notice.task, child_notice.route),
        (child_id, child_route)
    );
    owner.kernel.inbox.hub.enqueue(child_notice);
    assert!(owner.tick());
    assert!(child.completion().done());
    assert!(!parent.is_finished());
    assert_eq!(completion_selections.load(Ordering::Acquire), 0);
    assert_eq!(registration.publication(token), Publication::Published);
    assert_eq!(owner.kernel.parked.len(), 1);
    assert!(owner.kernel.ready.is_empty());
    writeln!(
        std::io::stdout().lock(),
        "deadline-join local={local} parent={parent_id:?} child={child_id:?} token={token:?} \
         publication=Published child_reclaimed=true parent_resumed=false completion_selections=0"
    )
    .unwrap();

    owner.kernel.inbox.hub.enqueue(held);
    owner.drain();
    let result = parent.take_result();
    writeln!(
        std::io::stdout().lock(),
        "deadline-join local={local} result={result:?}"
    )
    .unwrap();
    let report = result.expect("preserve parent panic or typed observations");
    report
        .observation
        .expect("join executed before local policy completion")
        .check();
    assert!(matches!(report.checkpoint, Err(Error::DeadlineExceeded)));
    if local {
        assert!(matches!(
            report.local_policy,
            Some(Err(Error::DeadlineExceeded))
        ));
        assert_eq!(report.borrowed, Some(42));
    } else {
        assert!(report.local_policy.is_none());
        assert!(report.borrowed.is_none());
    }
    assert_eq!(registration.publication(token), Publication::Stale);
    assert_eq!(owner.kernel.shared.snapshot().active, 0);
    assert_eq!(owner.kernel.parked.len(), 0);
    assert_eq!(owner.kernel.tasks.len(), 0);
    assert!(!owner.kernel.has_borrowed);
    assert_eq!(
        (owner.kernel.stats.parks, owner.kernel.stats.timeouts),
        (2, 2)
    );
    owner.kernel.shared.wait(owner.scope, None).unwrap();
    assert!(matches!(
        owner
            .kernel
            .shared
            .scope_options(owner.scope)
            .unwrap()
            .check(),
        Err(Error::DeadlineExceeded)
    ));
}
