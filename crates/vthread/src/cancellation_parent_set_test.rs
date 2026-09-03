use super::ParentSet;

#[test]
fn common_ancestry_stays_compact_and_many_parents_demote() {
    assert_eq!(
        std::mem::size_of::<ParentSet>(),
        2 * std::mem::size_of::<usize>()
    );
    let mut ids = ParentSet::default();
    assert!(ids.insert(7));
    assert!(!ids.insert(7));
    assert!(ids.insert(9));
    assert!(matches!(ids, ParentSet::Many(_)));
    assert!(ids.remove(&7));
    assert!(matches!(ids, ParentSet::One(9)));
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![9]);
}
