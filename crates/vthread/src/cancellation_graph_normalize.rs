//! Incremental relay compaction and exact ancestry indexing.

use super::{Graph, Kind, Signature};
use std::collections::BTreeSet;

impl Graph {
    pub(super) fn normalize<I>(&mut self, initial: I)
    where
        I: IntoIterator<Item = usize>,
    {
        let mut pending = initial.into_iter().collect::<BTreeSet<_>>();
        loop {
            let mut touched = BTreeSet::new();
            while let Some(id) = pending.pop_first() {
                let Some((parents, compact)) = self.nodes.get(&id).and_then(|entry| {
                    (entry.kind == Kind::Relay)
                        .then(|| (entry.parents.clone(), entry.children.len() <= 1))
                }) else {
                    continue;
                };
                if parents.len() <= 1 || compact {
                    pending.extend(self.erase(id, true));
                    continue;
                }
                let signature = self.signature_for(&parents);
                let changed = !self.nodes[&id].signature.same_set(&signature);
                if changed {
                    self.unindex_relay(id);
                    let children = {
                        let entry = self.nodes.get_mut(&id).expect("live relay");
                        entry.signature = signature;
                        entry.children.clone()
                    };
                    self.index_relay(id);
                    pending.extend(children);
                }
                touched.insert(id);
            }
            let duplicate = touched
                .into_iter()
                .find_map(|id| self.equivalent_relay(id).map(|other| (id, other)));
            let Some((left, right)) = duplicate else {
                break;
            };
            let (keep, remove) = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            pending.extend(self.merge_relays(keep, remove));
        }
    }

    pub(super) fn signature_for(&mut self, parents: &BTreeSet<usize>) -> Signature {
        let mut signature = Signature::default();
        for parent in parents {
            let next = self.nodes[parent].signature.clone();
            #[cfg(test)]
            {
                self.work.signature_items += signature.cardinality().min(next.cardinality());
            }
            signature = signature.union(&next);
        }
        signature
    }

    pub(super) fn index_relay(&mut self, id: usize) {
        let entry = self.nodes.get(&id).expect("live relay");
        assert!(entry.kind == Kind::Relay, "only relays are indexed");
        self.relay_index
            .entry(entry.signature.candidate())
            .or_default()
            .insert(id);
    }

    fn unindex_relay(&mut self, id: usize) {
        let candidate = self.nodes[&id].signature.candidate();
        let empty = {
            let ids = self
                .relay_index
                .get_mut(&candidate)
                .expect("indexed relay candidate");
            assert!(ids.remove(&id), "indexed relay identity");
            ids.is_empty()
        };
        if empty {
            self.relay_index.remove(&candidate);
        }
    }

    fn equivalent_relay(&mut self, id: usize) -> Option<usize> {
        let entry = self.nodes.get(&id)?;
        if entry.kind != Kind::Relay {
            return None;
        }
        let candidate = entry.signature.candidate();
        let cancelled = entry.cancelled;
        let signature = entry.signature.clone();
        #[cfg(test)]
        let mut compared = 0;
        let match_id = self
            .relay_index
            .get(&candidate)?
            .iter()
            .copied()
            .find(|other| {
                if *other == id {
                    return false;
                }
                let other_entry = self.nodes.get(other).expect("indexed relay");
                if other_entry.cancelled != cancelled {
                    return false;
                }
                #[cfg(test)]
                {
                    compared += signature.cardinality();
                }
                signature.same_set(&other_entry.signature)
            });
        #[cfg(test)]
        {
            self.work.exact_items += compared;
        }
        match_id
    }

    pub(super) fn erase(&mut self, id: usize, splice: bool) -> Vec<usize> {
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
        entry.parents.into_iter().chain(entry.children).collect()
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

    fn merge_relays(&mut self, keep: usize, remove: usize) -> Vec<usize> {
        {
            let retained = &self.nodes[&keep];
            let discarded = &self.nodes[&remove];
            assert!(retained.kind == Kind::Relay && discarded.kind == Kind::Relay);
            assert_eq!(retained.cancelled, discarded.cancelled);
            assert!(retained.signature.same_set(&discarded.signature));
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
            .collect()
    }
}

#[cfg(test)]
#[path = "cancellation_graph_normalize_test.rs"]
mod cancellation_graph_normalize_test;
