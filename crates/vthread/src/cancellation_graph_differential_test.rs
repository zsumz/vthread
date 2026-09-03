use super::{Graph, Kind, Signature};
use std::collections::BTreeSet;

#[derive(Default)]
struct Reference {
    nodes: Vec<ReferenceNode>,
}

struct ReferenceNode {
    cancelled: bool,
    children: Vec<usize>,
}

impl Reference {
    fn insert(&mut self, parents: &[usize]) -> usize {
        let id = self.nodes.len();
        let cancelled = parents.iter().any(|parent| self.nodes[*parent].cancelled);
        self.nodes.push(ReferenceNode {
            cancelled,
            children: Vec::new(),
        });
        for parent in parents {
            self.nodes[*parent].children.push(id);
        }
        id
    }

    fn cancel(&mut self, id: usize) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            if self.nodes[id].cancelled {
                continue;
            }
            self.nodes[id].cancelled = true;
            pending.extend(self.nodes[id].children.clone());
        }
    }
}

fn insert(graph: &mut Graph, reference: &mut Reference, parents: &[usize]) -> usize {
    let expected = reference.insert(parents);
    let actual = graph.insert_inert(parents);
    assert_eq!(actual, expected);
    actual
}

fn assert_equivalent(graph: &Graph, reference: &Reference, live: &[usize], step: usize) {
    for id in live {
        assert_eq!(
            graph.is_cancelled(*id),
            reference.nodes[*id].cancelled,
            "step={step} token={id}"
        );
    }
    let actual_relays = graph
        .nodes
        .values()
        .filter(|entry| entry.kind == Kind::Relay)
        .count();
    assert_eq!(actual_relays, graph.relays);
    for (id, entry) in &graph.nodes {
        if entry.kind == Kind::Relay {
            assert!(entry.parents.len() > 1, "step={step} unary parents at {id}");
            assert!(
                entry.children.len() > 1,
                "step={step} unary children at {id}"
            );
            let expected = entry
                .parents
                .iter()
                .fold(Signature::default(), |set, parent| {
                    set.union(&graph.nodes[parent].signature)
                });
            assert!(entry.signature.same_set(&expected));
            assert!(graph.relay_index[&entry.signature.candidate()].contains(id));
        } else {
            assert!(entry.signature.same_set(&Signature::singleton(*id)));
        }
        for parent in &entry.parents {
            let predecessor = &graph.nodes[parent];
            assert!(predecessor.children.contains(id));
            assert!(parent < id);
            assert!(!predecessor.cancelled || entry.cancelled);
        }
        for child in &entry.children {
            assert!(graph.nodes[child].parents.contains(id));
            assert!(id < child);
        }
    }
    for (candidate, ids) in &graph.relay_index {
        for id in ids {
            let entry = &graph.nodes[id];
            assert_eq!(*candidate, entry.signature.candidate());
            assert_eq!(entry.kind, Kind::Relay);
        }
    }
}

#[test]
fn compressed_graph_matches_a_non_pruning_reference_under_mixed_operations() {
    let mut graph = Graph::default();
    let mut reference = Reference::default();
    let mut live = Vec::new();
    let mut seed = 0x4d59_5df4_d0f3_3173_u64;

    for step in 0..5_000 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let choice = (seed >> 32) as usize % 100;
        if live.len() < 4 || (choice < 48 && live.len() < 48) {
            let count = ((seed >> 16) as usize % 3).min(live.len());
            let mut parents = BTreeSet::new();
            let mut cursor = seed;
            while parents.len() < count {
                cursor = cursor.rotate_left(17).wrapping_add(0x9e37_79b9_7f4a_7c15);
                parents.insert(live[(cursor as usize) % live.len()]);
            }
            let parents = parents.into_iter().collect::<Vec<_>>();
            live.push(insert(&mut graph, &mut reference, &parents));
        } else {
            let index = (seed as usize) % live.len();
            let id = live[index];
            if choice < 72 {
                graph.cancel(id);
                reference.cancel(id);
            } else {
                graph.remove(id);
                live.swap_remove(index);
            }
        }
        assert_equivalent(&graph, &reference, &live, step);
    }
}
