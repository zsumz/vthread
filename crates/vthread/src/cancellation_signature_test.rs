fn max_depth(signature: &Signature) -> usize {
    let root = match &signature.0 {
        Root::Empty => return 0,
        Root::Singleton(_) => return 1,
        Root::Tree(root) => root,
    };
    let mut depth = 0;
    let mut pending = vec![(root.as_ref(), 1)];
    while let Some((node, current)) = pending.pop() {
        depth = depth.max(current);
        if let NodeKind::Branch { left, right, .. } = &node.kind {
            pending.push((left.as_ref(), current + 1));
            pending.push((right.as_ref(), current + 1));
        }
    }
    depth
}

use super::{NodeKind, Root, Signature};

#[test]
fn singleton_signatures_stay_inline() {
    assert!(matches!(Signature::singleton(7).0, Root::Singleton(7)));
}

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
    assert!(max_depth(&forward) <= usize::BITS as usize + 1);
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
    assert!(max_depth(&all) <= usize::BITS as usize + 1);
    assert!(!even.same_set(&odd));
}

#[test]
fn structurally_shared_equality_visits_only_changed_paths() {
    let mut base = Signature::default();
    for id in 0..10_000 {
        base = base.union(&Signature::singleton(id));
    }
    let addition = Signature::singleton(20_000);
    let (left, left_work) = base.union_counted(&addition);
    let (right, right_work) = base.union_counted(&addition);
    assert_eq!(left_work.union_items, 1);
    assert_eq!(right_work.union_items, 1);
    assert!(left_work.allocated_nodes <= usize::BITS as usize + 2);
    assert!(right_work.allocated_nodes <= usize::BITS as usize + 2);

    let (same, comparison) = left.same_set_counted(&right);
    assert!(same);
    assert!(
        comparison.equality_nodes <= 2 * usize::BITS as usize + 3,
        "shared equality visited {} Patricia nodes",
        comparison.equality_nodes
    );
    assert_eq!(comparison.union_items, 0);
    assert_eq!(comparison.allocated_nodes, 0);
}
