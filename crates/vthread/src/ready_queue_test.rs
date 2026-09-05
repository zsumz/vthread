use crate::task_slab::TaskKey;

use super::{ReadyQueue, WAKE_BURST};

#[test]
fn replenished_hot_wakes_and_normal_work_cannot_starve_an_old_wake() {
    let mut ready = ReadyQueue::new();
    let old = TaskKey::owned(0);
    let hot_a = TaskKey::owned(1);
    let hot_b = TaskKey::owned(2);
    let normal = TaskKey::owned(3);
    ready.push_wake(old);
    ready.push_wake(hot_a);
    ready.push_back(normal);
    let (mut old_selected, mut normal_selected) = (0, 0);
    for _ in 0..usize::from(WAKE_BURST) + 2 {
        let task = ready.pop_front().expect("bounded runnable population");
        if task == old {
            old_selected += 1;
        } else if task == normal {
            normal_selected += 1;
            ready.push_back(normal);
        } else {
            assert!(task == hot_a || task == hot_b);
            ready.push_wake(if task == hot_a { hot_b } else { hot_a });
        }
        assert!(ready.len() <= 3);
    }
    assert_eq!(normal_selected, 1, "normal work lost its service quota");
    assert_eq!(old_selected, 1, "normal work erased the oldest-wake quota");
}

#[test]
fn every_cohort_phase_bounds_both_queues_under_replenishment() {
    for phase in 0..=WAKE_BURST + 1 {
        for wake_count in 1..=8 {
            for normal_count in 1..=8 {
                check_progress_bound(phase, wake_count, normal_count);
            }
        }
    }
}

fn check_progress_bound(phase: u8, wake_count: usize, normal_count: usize) {
    let mut ready = ReadyQueue::new();
    ready.wake_streak = phase;
    for index in 0..wake_count {
        ready.push_wake(TaskKey::owned(index));
    }
    for index in 0..normal_count {
        ready.push_back(TaskKey::owned(100 + index));
    }
    let (hot_a, hot_b) = (TaskKey::owned(64), TaskKey::owned(65));
    ready.push_wake(hot_a);
    let (mut wakes_seen, mut normals_seen) = ([false; 8], [false; 8]);
    let cohort = usize::from(WAKE_BURST) + 2;
    for dispatch in 1..=cohort * wake_count.max(normal_count) {
        let expected = ready.front().copied();
        let task = ready.pop_front().expect("live replenished population");
        assert_eq!(Some(task), expected, "peek disagreed with selection");
        if task == hot_a || task == hot_b {
            ready.push_wake(if task == hot_a { hot_b } else { hot_a });
        } else if let Some(index) = (0..wake_count).find(|i| task == TaskKey::owned(*i)) {
            assert!(!wakes_seen[index], "wake selected twice");
            wakes_seen[index] = true;
        } else {
            let index = (0..normal_count)
                .find(|i| task == TaskKey::owned(100 + *i))
                .expect("known normal task");
            normals_seen[index] = true;
            ready.push_back(task);
        }
        assert!(ready.len() <= wake_count + normal_count + 1);
        for (index, seen) in wakes_seen.iter().enumerate().take(wake_count) {
            if dispatch >= cohort * (index + 1) {
                assert!(
                    seen,
                    "wake {index} missed its bound: phase={phase}, wakes={wake_count}, normals={normal_count}"
                );
            }
        }
        for (index, seen) in normals_seen.iter().enumerate().take(normal_count) {
            if dispatch >= cohort * (index + 1) {
                assert!(
                    seen,
                    "normal {index} missed its bound: phase={phase}, wakes={wake_count}, normals={normal_count}"
                );
            }
        }
    }
}

#[test]
fn normal_only_work_does_not_retain_an_empty_wake_cohort() {
    let mut ready = ReadyQueue::new();
    let normal = TaskKey::owned(0);
    for phase in 0..=WAKE_BURST + 1 {
        ready.wake_streak = phase;
        ready.push_back(normal);
        assert_eq!(ready.front(), Some(&normal));
        assert_eq!(ready.pop_front(), Some(normal));
        assert_eq!(ready.wake_streak, 0);
        assert_eq!(ready.front(), None);
        assert_eq!(ready.pop_front(), None);
    }
    ready.push_wake(TaskKey::owned(1));
    ready.push_wake(TaskKey::owned(2));
    assert_eq!(ready.pop_front(), Some(TaskKey::owned(2)));
}

#[test]
fn selected_wakes_get_latency_priority_with_a_fixed_starvation_bound() {
    let mut ready = ReadyQueue::new();
    let normal = TaskKey::owned(0);
    ready.push_back(normal);
    let burst = usize::from(WAKE_BURST);
    for index in 1..=burst + 1 {
        ready.push_wake(TaskKey::owned(index));
    }
    for expected in (2..=burst + 1).rev() {
        assert_eq!(ready.pop_front(), Some(TaskKey::owned(expected)));
    }
    assert_eq!(ready.pop_front(), Some(normal));
    assert_eq!(ready.pop_front(), Some(TaskKey::owned(1)));
}

#[test]
fn wake_only_pressure_periodically_selects_the_oldest_generation() {
    let mut ready = ReadyQueue::new();
    let burst = usize::from(WAKE_BURST);
    for index in 0..=burst {
        ready.push_wake(TaskKey::owned(index));
    }
    for expected in (1..=burst).rev() {
        assert_eq!(ready.pop_front(), Some(TaskKey::owned(expected)));
    }
    assert_eq!(ready.pop_front(), Some(TaskKey::owned(0)));
}
