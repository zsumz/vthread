use std::time::{Duration, Instant};

use crate::task_slab::TaskKey;
use vthread_stack::ParkToken;

use super::{ExpiredTimer, TimerQueue};

fn expiry(task: TaskKey, token: ParkToken) -> ExpiredTimer {
    ExpiredTimer { task, token }
}

#[test]
fn deadlines_are_returned_in_monotonic_order() {
    let now = Instant::now();
    let early = ParkToken::new(1, 1);
    let late = ParkToken::new(2, 1);
    let early_task = TaskKey::owned(1);
    let late_task = TaskKey::owned(2);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(late_task, late, now + Duration::from_secs(2)));
    assert!(timers.schedule(early_task, early, now + Duration::from_secs(1)));

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(1)));
    assert_eq!(
        timers.pop_expired(now + Duration::from_millis(1500)),
        vec![expiry(early_task, early)]
    );
    assert_eq!(timers.active_count(), 1);
    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(3)),
        vec![expiry(late_task, late)]
    );
}

#[test]
fn cancellation_removes_the_earliest_deadline() {
    let now = Instant::now();
    let cancelled = ParkToken::new(1, 1);
    let active = ParkToken::new(2, 1);
    let task = TaskKey::owned(0);
    let mut timers = TimerQueue::new();
    timers.schedule(task, cancelled, now + Duration::from_secs(1));
    timers.schedule(task, active, now + Duration::from_secs(2));
    assert!(timers.cancel(cancelled));

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(2)));
    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(3)),
        vec![expiry(task, active)]
    );
    assert_eq!(timers.active_count(), 0);
}

#[test]
fn cancellation_releases_storage_behind_a_live_earlier_deadline() {
    let now = Instant::now();
    let early = ParkToken::new(1, 1);
    let task = TaskKey::owned(0);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(task, early, now + Duration::from_secs(1)));

    for generation in 1..=256 {
        let later = ParkToken::new(2, generation);
        assert!(timers.schedule(task, later, now + Duration::from_secs(60)));
        assert!(timers.cancel(later));
    }

    assert_eq!(timers.next_deadline(), Some(now + Duration::from_secs(1)));
    assert_eq!(timers.active_count(), 1);
    assert_eq!(
        timers.deadlines.len(),
        1,
        "cancelled entries must not accumulate"
    );
    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(60)),
        vec![expiry(task, early)]
    );
}

#[test]
fn duplicate_registration_preserves_the_original_deadline() {
    let now = Instant::now();
    let token = ParkToken::new(1, 1);
    let task = TaskKey::owned(0);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(task, token, now + Duration::from_secs(1)));
    assert!(!timers.schedule(task, token, now + Duration::from_secs(2)));

    assert_eq!(
        timers.pop_expired(now + Duration::from_secs(1)),
        vec![expiry(task, token)]
    );
    assert!(timers.pop_expired(now + Duration::from_secs(2)).is_empty());
    assert_eq!(timers.active_count(), 0);
}

#[test]
fn cancellation_is_exact_for_tokens_sharing_a_deadline() {
    let now = Instant::now();
    let old = ParkToken::new(1, 1);
    let current = ParkToken::new(1, 2);
    let other = ParkToken::new(2, 1);
    let old_task = TaskKey::owned(0);
    let current_task = TaskKey::owned(1);
    let other_task = TaskKey::borrowed(0);
    let mut timers = TimerQueue::new();
    assert!(timers.schedule(old_task, old, now));
    assert!(timers.schedule(current_task, current, now));
    assert!(timers.schedule(other_task, other, now));
    assert!(timers.cancel(old));
    assert!(!timers.cancel(old));

    assert_eq!(
        timers.pop_expired(now),
        vec![expiry(current_task, current), expiry(other_task, other)]
    );
    assert_eq!(timers.active_count(), 0);
    assert_eq!(timers.next_deadline(), None);
}

#[test]
fn duplicate_registration_cannot_replace_the_original_route() {
    let now = Instant::now();
    let token = ParkToken::new(1, 1);
    let original = TaskKey::owned(7);
    let stale = TaskKey::borrowed(7);
    let mut timers = TimerQueue::new();

    assert!(timers.schedule(original, token, now));
    assert!(!timers.schedule(stale, token, now));
    assert_eq!(timers.pop_expired(now), vec![expiry(original, token)]);
}
