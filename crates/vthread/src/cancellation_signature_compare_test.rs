use super::super::Signature;

#[test]
fn equality_stops_at_shared_subtrees() {
    let mut base = Signature::default();
    for id in 0..1_000 {
        base = base.union(&Signature::singleton(id));
    }
    let addition = Signature::singleton(2_000);
    let left = base.union(&addition);
    let right = base.union(&addition);
    let (same, work) = left.same_set_counted(&right);
    assert!(same);
    assert!(work.equality_nodes <= 2 * usize::BITS as usize + 3);
}
