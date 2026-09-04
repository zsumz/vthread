use super::pair_indices;

#[test]
fn adjacent_task_owners_form_benchmark_pairs() {
    assert_eq!(
        pair_indices(&[0, 1, 2, 3, 3, 3]),
        vec![(0, 1), (2, 3), (3, 3)]
    );
}

#[test]
#[should_panic(expected = "task pairs must be complete")]
fn incomplete_pairs_are_rejected() {
    let _ = pair_indices(&[0, 1, 2]);
}
