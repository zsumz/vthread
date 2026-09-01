use super::super::Graph;
use std::sync::{Arc, atomic::AtomicBool};

fn insert(graph: &mut Graph, parents: &[usize]) -> usize {
    graph.insert(parents, Arc::new(AtomicBool::new(false)))
}

#[test]
fn merging_a_relay_rehomes_descendants_without_losing_cancellation() {
    let mut graph = Graph::default();
    let roots = [insert(&mut graph, &[]), insert(&mut graph, &[])];
    let retained = insert(&mut graph, &roots);
    let retained_children = [
        insert(&mut graph, &[retained]),
        insert(&mut graph, &[retained]),
    ];
    graph.remove(retained);

    let predecessor = insert(&mut graph, &roots);
    let side = insert(&mut graph, &[predecessor]);
    let duplicate = insert(&mut graph, &[predecessor]);
    graph.remove(predecessor);
    let duplicate_children = [
        insert(&mut graph, &[duplicate]),
        insert(&mut graph, &[duplicate]),
    ];
    graph.remove(duplicate);

    assert!(!graph.nodes.contains_key(&predecessor));
    assert!(graph.nodes[&side].parents.contains(&retained));
    graph.cancel(roots[0]);
    assert!(
        retained_children
            .iter()
            .chain(&duplicate_children)
            .all(|id| graph.is_cancelled(*id))
    );
}
