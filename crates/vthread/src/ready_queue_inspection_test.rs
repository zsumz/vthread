use super::super::{ReadyQueue, WAKE_BURST};
use crate::task_slab::TaskKey;

fn queue(normal: usize, wakes: usize, phase: u8) -> ReadyQueue {
    let mut queue = ReadyQueue::new();
    for index in 0..normal {
        queue.push_back(TaskKey::owned(index));
    }
    for index in normal..normal + wakes {
        queue.push_wake(TaskKey::owned(index));
    }
    queue.wake_streak = phase;
    queue
}

#[test]
fn inspection_visits_each_entry_once_and_preserves_retained_lanes() {
    for phase in 0..=WAKE_BURST + 1 {
        for normal in 0..=4 {
            for wakes in 0..=4 {
                for mask in 0..1_usize << (normal + wakes) {
                    let mut queue = queue(normal, wakes, phase);
                    let selected = |task: TaskKey| mask & (1 << task.index()) != 0;
                    let mut expected_normal = queue.normal.clone();
                    expected_normal.retain(|task| !selected(*task));
                    let mut expected_wakes = queue.wakes.clone();
                    expected_wakes.retain(|task| !selected(*task));
                    let mut visits = [0; 8];
                    let mut removed = Vec::new();
                    let mut inspection = queue.inspection();
                    while let Some(task) = queue.remove_matching(&mut inspection, |task| {
                        visits[task.index()] += 1;
                        selected(task)
                    }) {
                        assert!(!removed.contains(&task));
                        removed.push(task);
                        assert_eq!(queue.len() + removed.len(), normal + wakes);
                        assert_eq!(queue.wake_streak, phase);
                    }
                    assert_eq!(visits[..normal + wakes], vec![1; normal + wakes]);
                    assert_eq!(removed.len(), mask.count_ones() as usize);
                    assert_eq!(queue.normal, expected_normal);
                    assert_eq!(queue.wakes, expected_wakes);
                    assert_eq!(queue.wake_streak, phase);
                }
            }
        }
    }
}

#[test]
fn eligible_selection_matches_the_dispatch_policy_without_moving_skipped_entries() {
    for phase in 0..=WAKE_BURST + 1 {
        for normal in 0..=4 {
            for wakes in 0..=4 {
                for mask in 0..1_usize << (normal + wakes) {
                    let mut actual = queue(normal, wakes, phase);
                    let mut reference = queue(normal, wakes, phase);
                    let eligible = |task: TaskKey| mask & (1 << task.index()) != 0;
                    reference.normal.retain(|task| eligible(*task));
                    reference.wakes.retain(|task| eligible(*task));
                    let expected = reference.pop_front();
                    let mut expected_normal = actual.normal.clone();
                    expected_normal.retain(|task| Some(*task) != expected);
                    let mut expected_wakes = actual.wakes.clone();
                    expected_wakes.retain(|task| Some(*task) != expected);
                    let mut visits = [0; 8];

                    let selected = actual.pop_matching(|task| {
                        visits[task.index()] += 1;
                        eligible(task)
                    });

                    assert_eq!(selected, expected);
                    assert!(visits.iter().all(|visits| *visits <= 1));
                    assert_eq!(actual.normal, expected_normal);
                    assert_eq!(actual.wakes, expected_wakes);
                    assert_eq!(
                        actual.wake_streak,
                        if selected.is_some() {
                            reference.wake_streak
                        } else {
                            phase
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn identity_removal_preserves_other_entries_and_cohort_state() {
    for phase in 0..=WAKE_BURST + 1 {
        for index in 0..=8 {
            let mut queue = queue(4, 4, phase);
            let task = TaskKey::owned(index);
            let mut normal = queue.normal.clone();
            normal.retain(|entry| *entry != task);
            let mut wakes = queue.wakes.clone();
            wakes.retain(|entry| *entry != task);

            assert_eq!(queue.remove(task), index < 8);

            assert_eq!(queue.normal, normal);
            assert_eq!(queue.wakes, wakes);
            assert_eq!(queue.wake_streak, phase);
        }
    }
}
