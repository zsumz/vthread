use crate::task_slab::TaskKey;

use super::ReadyQueue;

#[test]
fn selected_wakes_get_latency_priority_with_a_fixed_starvation_bound() {
    let mut ready = ReadyQueue::new();
    let normal = TaskKey::owned(0);
    ready.push_back(normal);
    for index in 1..=33 {
        ready.push_wake(TaskKey::owned(index));
    }
    for expected in (2..=33).rev() {
        assert_eq!(ready.pop_front(), Some(TaskKey::owned(expected)));
    }
    assert_eq!(ready.pop_front(), Some(normal));
    assert_eq!(ready.pop_front(), Some(TaskKey::owned(1)));
}

#[test]
fn wake_only_pressure_periodically_selects_the_oldest_generation() {
    let mut ready = ReadyQueue::new();
    for index in 0..=32 {
        ready.push_wake(TaskKey::owned(index));
    }
    for expected in (1..=32).rev() {
        assert_eq!(ready.pop_front(), Some(TaskKey::owned(expected)));
    }
    assert_eq!(ready.pop_front(), Some(TaskKey::owned(0)));
}
