//! Select an exact timer generation before permitting delayed stack resumption.

use crate::{
    CarrierId, Error, JoinHandle, ParkOutcome, Runtime, ScopeOptions, TaskFailure, control::Shared,
    kernel::Kernel, wait::Publication,
};
use std::{
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

struct Owner {
    kernel: Kernel,
    scope: u64,
}

impl Drop for Owner {
    fn drop(&mut self) {
        while !self.kernel.abort(None, TaskFailure::RuntimeStopped) {
            std::thread::yield_now();
        }
        self.kernel.shared.finish_scope(self.scope);
    }
}

fn wait_until(deadline: Instant) {
    // Only the ordinary test driver waits here, never a mounted vthread. A
    // spurious native wake rechecks the clock; it cannot substitute for expiry.
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        std::thread::park_timeout(remaining);
    }
}

#[test]
fn a_selected_timer_preserves_which_deadline_won_after_delayed_resume() {
    for explicit_offset in [-25i64, 0, 25] {
        selected_timer(explicit_offset);
    }
}

fn owner() -> (Owner, Instant) {
    let config = Runtime::builder()
        .max_vthreads(4)
        .stack_cache_capacity(4)
        .carrier_queue_capacity(4)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    let deadline = Instant::now() + Duration::from_secs(1);
    let scope = shared
        .begin_owned(ScopeOptions::default().deadline(deadline), false)
        .unwrap();
    (Owner { kernel, scope }, deadline)
}

fn selected_timer(explicit_offset: i64) {
    let (mut owner, deadline) = owner();
    let shared = Arc::clone(&owner.kernel.shared);
    let scope = owner.scope;
    let explicit = if explicit_offset < 0 {
        deadline - Duration::from_millis(explicit_offset.unsigned_abs())
    } else {
        deadline + Duration::from_millis(explicit_offset as u64)
    };
    let (parker, _waker) = crate::park_pair();
    let wait = parker.wait.clone();
    let task = shared
        .submit(scope, "ordered timer winner".into(), move || {
            (parker.park_until(explicit), crate::checkpoint())
        })
        .unwrap();
    let mut handle = JoinHandle::new(Arc::clone(&shared), task.id, task.cell, task.record);
    assert!(!owner.kernel.receive());
    let _route = crate::context::mount_carrier(&owner.kernel.inbox.hub, &owner.kernel.local);
    assert!(owner.kernel.tick(false).unwrap());
    let parked = owner.kernel.parked.iter().next().unwrap_or_else(|| {
        panic!("timer never parked: result={:?}", handle.take_result());
    });
    let (route, token) = (parked.task, parked.token);
    let due = explicit.min(deadline);
    assert_eq!(owner.kernel.timers.next_deadline(), Some(due));
    assert_eq!(owner.kernel.stats.parks, 1);

    wait_until(due);
    owner.kernel.expire_timers().unwrap();
    let published = wait.publication(token);
    let selected_at = Instant::now();
    assert_eq!(
        published,
        Publication::Published,
        "the exact timer must publish selection before any resumption"
    );
    assert_eq!(owner.kernel.timers.active_count(), 0);
    assert_eq!(owner.kernel.parked.get(route).unwrap().token, token);
    assert!(owner.kernel.ready.is_empty());
    assert!(!handle.is_finished());
    assert!(!wait.registration().select_ready(token));

    // Selection is already observed, not inferred from this elapsed interval.
    // For the earlier explicit timer this postpones resume past inherited expiry.
    wait_until(deadline);
    let resume_after = Instant::now();
    assert!(owner.kernel.tick(true).unwrap());
    while owner.kernel.tick(true).unwrap() {}
    let result = handle.take_result(); // Preserve typed outcome / task panic evidence.
    let drained = shared.wait(scope, None);
    let scope_result = shared.scope_options(scope).unwrap().check();
    let stats = owner.kernel.stats;
    assert_eq!(wait.publication(token), Publication::Stale);
    drop(owner);

    writeln!(
        std::io::stdout().lock(),
        "timer-selection offset_ms={explicit_offset} route={route:?} token={token:?} \
         publication={published:?} selected_at={selected_at:?} inherited={deadline:?} \
         resume_after={resume_after:?} result={result:?} scope_policy={scope_result:?}"
    )
    .unwrap();

    let (outcome, checkpoint) = result.expect("selected task returned its typed outcome");
    if explicit_offset < 0 {
        assert!(matches!(outcome, Ok(ParkOutcome::TimedOut)), "{outcome:?}");
    } else {
        assert!(
            matches!(outcome, Err(Error::DeadlineExceeded)),
            "{outcome:?}"
        );
    }
    assert!(matches!(checkpoint, Err(Error::DeadlineExceeded)));
    drained.expect("all admitted work completed before scope policy observation");
    assert!(matches!(scope_result, Err(Error::DeadlineExceeded)));
    assert_eq!((stats.parks, stats.wakes, stats.timeouts), (1, 1, 1));
    assert_eq!(shared.snapshot().active, 0);
}

#[test]
fn admission_does_not_prove_the_timer_task_ever_parked() {
    let (mut owner, deadline) = owner();
    let shared = Arc::clone(&owner.kernel.shared);
    let task = shared
        .submit(owner.scope, "accepted before expiry".into(), move || {
            let (parker, _waker) = crate::park_pair();
            parker.park_until(deadline)
        })
        .unwrap();
    let mut handle = JoinHandle::new(Arc::clone(&shared), task.id, task.cell, task.record);
    assert!(!handle.is_finished());
    wait_until(deadline); // An accepted packet need not mount before its deadline.
    assert!(!owner.kernel.receive());
    while owner.kernel.tick(true).unwrap() {}
    let result = handle.take_result();
    let parks = owner.kernel.stats.parks;
    let pending = owner.kernel.parked.len();
    assert_eq!(owner.kernel.timers.active_count(), 0);
    drop(owner);
    writeln!(
        std::io::stdout().lock(),
        "timer-admission parks={parks} pending={pending} result={result:?}"
    )
    .unwrap();
    assert!(
        matches!(result, Ok(Err(Error::DeadlineExceeded))),
        "{result:?}"
    );
    assert_eq!((parks, pending), (0, 0));
    assert_eq!(shared.snapshot().active, 0);
}
