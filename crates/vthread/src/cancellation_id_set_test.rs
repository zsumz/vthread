use super::IdSet;

#[test]
fn singleton_edges_stay_inline_and_many_edges_demote() {
    let mut ids = IdSet::default();
    assert!(ids.insert(7));
    assert!(matches!(ids, IdSet::One(7)));
    assert!(!ids.insert(7));
    assert!(ids.insert(9));
    assert!(matches!(ids, IdSet::Many(_)));
    assert!(ids.remove(&7));
    assert!(matches!(ids, IdSet::One(9)));
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![9]);
}
