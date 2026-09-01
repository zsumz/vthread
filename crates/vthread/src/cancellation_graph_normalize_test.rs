use super::super::Graph;
use std::sync::{Arc, atomic::AtomicBool};

fn insert(graph: &mut Graph, parents: &[usize]) -> usize {
    graph.insert(parents, Arc::new(AtomicBool::new(false)))
}

#[test]
fn equivalent_relays_merge_only_when_cancellation_state_matches() {
    let mut graph = Graph::default();
    let roots = [insert(&mut graph, &[]), insert(&mut graph, &[])];
    let retired = insert(&mut graph, &[]);
    let old = insert(&mut graph, &[roots[0], roots[1], retired]);
    let old_children = [insert(&mut graph, &[old]), insert(&mut graph, &[old])];
    graph.cancel(retired);
    graph.remove(old);
    graph.remove(retired);

    let fresh = insert(&mut graph, &roots);
    let fresh_children = [insert(&mut graph, &[fresh]), insert(&mut graph, &[fresh])];
    graph.remove(fresh);

    assert_eq!(graph.snapshot().1, 2);
    assert!(old_children.iter().all(|id| graph.is_cancelled(*id)));
    assert!(fresh_children.iter().all(|id| !graph.is_cancelled(*id)));
    graph.cancel(roots[0]);
    assert!(fresh_children.iter().all(|id| graph.is_cancelled(*id)));
}

#[test]
fn merging_a_descendant_relay_reconsiders_its_predecessor() {
    let mut graph = Graph::default();
    let roots = [
        insert(&mut graph, &[]),
        insert(&mut graph, &[]),
        insert(&mut graph, &[]),
    ];

    let prefix = insert(&mut graph, &roots[..2]);
    let retained = insert(&mut graph, &[prefix, roots[2]]);
    graph.remove(prefix);
    let retained_children = [
        insert(&mut graph, &[retained]),
        insert(&mut graph, &[retained]),
    ];
    graph.remove(retained);

    let predecessor = insert(&mut graph, &roots[..2]);
    let side = insert(&mut graph, &[predecessor]);
    let duplicate = insert(&mut graph, &[predecessor, roots[2]]);
    graph.remove(predecessor);
    let duplicate_children = [
        insert(&mut graph, &[duplicate]),
        insert(&mut graph, &[duplicate]),
    ];
    graph.remove(duplicate);

    assert!(!graph.nodes.contains_key(&predecessor));
    assert!(
        roots[..2]
            .iter()
            .all(|root| graph.nodes[&side].parents.contains(root))
    );
    assert_eq!(graph.snapshot().1, 1);
    graph.cancel(roots[0]);
    assert!(
        retained_children
            .iter()
            .chain(&duplicate_children)
            .all(|id| graph.is_cancelled(*id))
    );
}
