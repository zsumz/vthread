use super::*;

#[test]
fn proc_state_handles_spaces_and_parentheses_in_names() {
    assert_eq!(state("12 (a weird) name) S 1 2 3").unwrap(), 'S');
    assert_eq!(state("12 (thread) R 1 2 3").unwrap(), 'R');
    assert!(state("12 (broken").is_err());
    assert!(state("12 (broken) 123").is_err());
}

#[test]
fn cpu_selection_never_reuses_or_exceeds_the_allowed_set() {
    assert_eq!(cpus("2-3,5,7-9", 3).unwrap(), [2, 3, 5]);
    for (list, count) in [("0-1", 3), ("2-1", 1), ("0,0", 1), ("", 1), ("0", 0)] {
        assert!(cpus(list, count).is_err());
    }
}

#[test]
fn carrier_names_may_become_visible_in_separate_startup_steps() {
    assert!(!population_ready(0, 2).unwrap());
    assert!(!population_ready(1, 2).unwrap());
    assert!(population_ready(2, 2).unwrap());
    assert!(population_ready(3, 2).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn actual_current_thread_is_not_a_sleeping_futex_waiter() {
    assert!(tid().unwrap() > 0);
    assert!(!sleeping(tid().unwrap()).unwrap());
}
