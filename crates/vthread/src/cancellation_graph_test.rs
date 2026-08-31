use super::Graph;
use std::sync::{Arc, atomic::AtomicBool};

fn insert(graph: &mut Graph, parents: &[usize]) -> usize {
    graph.insert(parents, Arc::new(AtomicBool::new(false)))
}

#[test]
fn pruning_preserves_diamond_ancestry_without_cancelling_siblings() {
    for cancelled_parent in [0, 1] {
        let mut graph = Graph::default();
        let roots = [insert(&mut graph, &[]), insert(&mut graph, &[])];
        let intermediate = insert(&mut graph, &roots);
        let left = insert(&mut graph, &[intermediate, roots[0]]);
        let right = insert(&mut graph, &[intermediate]);
        let unrelated = insert(&mut graph, &[]);
        graph.remove(intermediate);
        assert_eq!(graph.snapshot(), (5, 4));
        graph.cancel(roots[cancelled_parent]);
        assert!(graph.is_cancelled(left));
        assert!(graph.is_cancelled(right));
        assert!(!graph.is_cancelled(roots[1 - cancelled_parent]));
        assert!(!graph.is_cancelled(unrelated));
    }
}

#[test]
fn cancellation_and_reclamation_of_a_live_deep_chain_are_iterative() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut graph = Graph::default();
            let root = insert(&mut graph, &[]);
            let mut previous = root;
            for _ in 0..100_000 {
                previous = insert(&mut graph, &[previous]);
            }
            graph.cancel(root);
            assert!(graph.is_cancelled(previous));
            for id in 0..100_000 {
                graph.remove(id);
            }
            assert_eq!(graph.snapshot(), (1, 0));
            graph.remove(previous);
            assert_eq!(graph.snapshot(), (0, 0));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn every_pruning_subset_preserves_reachability_in_a_branching_owner_graph() {
    let parents: [&[usize]; 8] = [&[], &[0], &[0], &[1, 2], &[1], &[3], &[4, 5], &[2]];
    for removed in 0..256 {
        for cancelled in 0..8 {
            if removed & (1 << cancelled) != 0 {
                continue;
            }
            let mut graph = Graph::default();
            for links in parents {
                insert(&mut graph, links);
            }
            for id in 0..8 {
                if removed & (1 << id) != 0 {
                    graph.remove(id);
                }
            }
            graph.cancel(cancelled);
            let mut expected = [false; 8];
            for id in 0..8 {
                expected[id] = id == cancelled || parents[id].iter().any(|p| expected[*p]);
                if removed & (1 << id) == 0 {
                    assert_eq!(
                        graph.is_cancelled(id),
                        expected[id],
                        "mask={removed} cancel={cancelled} node={id}"
                    );
                }
            }
            assert_eq!(
                graph.nodes.len(),
                8 - (removed as u32).count_ones() as usize
            );
            for (id, node) in &graph.nodes {
                for child in &node.children {
                    assert!(graph.nodes[child].parents.contains(id));
                    assert!(child > id);
                }
            }
        }
    }
}
