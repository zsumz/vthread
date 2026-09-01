//! Topology rewrites after token pruning and relay equivalence.

use super::{Graph, Kind, normalize::RelayWork};
use std::collections::BTreeSet;

impl Graph {
    pub(super) fn erase(
        &mut self,
        id: usize,
        splice: bool,
        descendants_dirty: bool,
    ) -> Vec<RelayWork> {
        if self.nodes[&id].kind == Kind::Relay {
            self.unindex_relay(id);
        }
        let entry = self.nodes.remove(&id).expect("live graph entry");
        if entry.kind == Kind::Relay {
            self.relays -= 1;
        }
        for parent in &entry.parents {
            self.nodes
                .get_mut(parent)
                .expect("live predecessor")
                .children
                .remove(&id);
        }
        for child in &entry.children {
            self.nodes
                .get_mut(child)
                .expect("live descendant")
                .parents
                .remove(&id);
        }
        if splice {
            self.splice(&entry.parents, &entry.children);
        }
        let children = entry.children.into_iter().map(|id| {
            if descendants_dirty {
                RelayWork::dirty(id)
            } else {
                RelayWork::known(id)
            }
        });
        entry
            .parents
            .into_iter()
            .map(RelayWork::known)
            .chain(children)
            .collect()
    }

    fn splice(&mut self, parents: &BTreeSet<usize>, children: &BTreeSet<usize>) {
        for parent in parents {
            for child in children {
                if self
                    .nodes
                    .get_mut(parent)
                    .expect("live predecessor")
                    .children
                    .insert(*child)
                {
                    self.nodes
                        .get_mut(child)
                        .expect("live descendant")
                        .parents
                        .insert(*parent);
                }
            }
        }
    }

    pub(super) fn merge_relays(&mut self, keep: usize, remove: usize) -> Vec<RelayWork> {
        {
            // `equivalent_relay` already performed the collision-safe exact comparison.
            let retained = &self.nodes[&keep];
            let discarded = &self.nodes[&remove];
            assert!(retained.kind == Kind::Relay && discarded.kind == Kind::Relay);
            assert_eq!(retained.cancelled, discarded.cancelled);
            assert_eq!(
                retained.signature.candidate(),
                discarded.signature.candidate()
            );
        }
        self.unindex_relay(remove);
        let entry = self.nodes.remove(&remove).expect("duplicate relay");
        self.relays -= 1;
        for parent in &entry.parents {
            self.nodes
                .get_mut(parent)
                .expect("live predecessor")
                .children
                .remove(&remove);
        }
        for child in &entry.children {
            let node = self.nodes.get_mut(child).expect("live descendant");
            node.parents.remove(&remove);
            node.parents.insert(keep);
        }
        self.nodes
            .get_mut(&keep)
            .expect("retained relay")
            .children
            .extend(&entry.children);
        entry
            .parents
            .into_iter()
            .chain(entry.children)
            .chain([keep])
            .map(RelayWork::known)
            .collect()
    }
}

#[cfg(test)]
#[path = "cancellation_graph_rewrite_test.rs"]
mod cancellation_graph_rewrite_test;
