use super::Signature;

#[test]
fn insertion_order_does_not_change_exact_set_identity() {
    let mut forward = Signature::default();
    let mut reverse = Signature::default();
    for id in 0..1_000 {
        forward = forward.union(&Signature::singleton(id));
    }
    for id in (0..1_000).rev() {
        reverse = reverse.union(&Signature::singleton(id));
    }
    assert_eq!(forward.candidate(), reverse.candidate());
    assert!(forward.same_set(&reverse));
    assert!(forward.max_depth() <= usize::BITS as usize + 1);
}

#[test]
fn union_is_exact_idempotent_and_structurally_bounded() {
    let mut even = Signature::default();
    let mut odd = Signature::default();
    for id in 0..10_000 {
        let item = Signature::singleton(id);
        if id % 2 == 0 {
            even = even.union(&item);
        } else {
            odd = odd.union(&item);
        }
    }
    let all = even.union(&odd);
    assert_eq!(all.cardinality(), 10_000);
    assert!(all.same_set(&all.union(&even)));
    assert!(all.max_depth() <= usize::BITS as usize + 1);
    assert!(!even.same_set(&odd));
}
