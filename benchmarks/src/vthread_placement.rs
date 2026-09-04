use vthread::diagnostics::RuntimeSnapshot;

pub(crate) fn pair_owners(
    snapshot: &RuntimeSnapshot,
    expected_tasks: usize,
) -> Vec<(usize, usize)> {
    let owners = snapshot
        .tasks()
        .iter()
        .map(|task| task.carrier().index())
        .collect::<Vec<_>>();
    assert_eq!(
        owners.len(),
        expected_tasks,
        "placement snapshot must contain every benchmark task"
    );
    pair_indices(&owners)
}

fn pair_indices(owners: &[usize]) -> Vec<(usize, usize)> {
    assert!(
        owners.len().is_multiple_of(2),
        "task pairs must be complete"
    );
    owners
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

#[cfg(test)]
#[path = "vthread_placement_test.rs"]
mod vthread_placement_test;
