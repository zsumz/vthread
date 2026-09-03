use super::IdMap;

#[test]
fn monotonic_integer_keys_round_trip_without_aliasing() {
    let mut ids = IdMap::default();
    for id in 0_u64..10_000 {
        assert!(ids.insert(id, id + 1).is_none());
    }
    for id in 0_u64..10_000 {
        assert_eq!(ids.remove(&id), Some(id + 1));
    }
    assert!(ids.is_empty());
}
