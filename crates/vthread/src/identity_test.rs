use super::*;
#[test]
fn concurrent_runtime_ids_are_unique() {
    let workers: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| (0..100).map(|_| RuntimeId::next()).collect::<Vec<_>>()))
        .collect();
    let ids: std::collections::BTreeSet<_> = workers
        .into_iter()
        .flat_map(|w| w.join().unwrap())
        .collect();
    assert_eq!(ids.len(), 800);
}
