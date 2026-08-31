//! Compressed cancellation graph; callers serialize every transition.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

enum Kind {
    Token(Arc<AtomicBool>),
    Relay,
}

struct Entry {
    kind: Kind,
    cancelled: bool,
    parents: BTreeSet<usize>,
    children: BTreeSet<usize>,
}

#[derive(Default)]
pub(super) struct Graph {
    next: usize,
    relays: usize,
    nodes: BTreeMap<usize, Entry>,
}

impl Graph {
    pub(super) fn insert(&mut self, parents: &[usize], flag: Arc<AtomicBool>) -> usize {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("cancellation identity exhausted");
        let parents = parents.iter().copied().collect::<BTreeSet<_>>();
        let cancelled = parents.iter().any(|parent| self.nodes[parent].cancelled);
        flag.store(cancelled, Ordering::Release);
        for parent in &parents {
            self.nodes
                .get_mut(parent)
                .expect("live parent")
                .children
                .insert(id);
        }
        self.nodes.insert(
            id,
            Entry {
                kind: Kind::Token(flag),
                cancelled,
                parents,
                children: BTreeSet::new(),
            },
        );
        id
    }

    pub(super) fn remove(&mut self, id: usize) {
        let inherited = {
            let entry = &self.nodes[&id];
            entry
                .parents
                .iter()
                .any(|parent| self.nodes[parent].cancelled)
        };
        let entry = self.nodes.get_mut(&id).expect("live cancellation node");
        assert!(matches!(entry.kind, Kind::Token(_)), "token removed twice");
        if entry.parents.len() > 1 && entry.children.len() > 1 {
            entry.kind = Kind::Relay;
            self.relays += 1;
            // Cancellation requested directly on this token is already present in
            // its current descendants. Once the token dies, only live ancestors
            // may select future paths through this anonymous relay.
            entry.cancelled = inherited;
            self.normalize(vec![id], true);
        } else {
            let affected = self.erase(id, true);
            self.normalize(affected, false);
        }
    }

    fn erase(&mut self, id: usize, splice: bool) -> Vec<usize> {
        let entry = self.nodes.remove(&id).expect("live graph entry");
        if matches!(entry.kind, Kind::Relay) {
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
            for parent in &entry.parents {
                for child in &entry.children {
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
        entry.parents.into_iter().chain(entry.children).collect()
    }

    fn normalize(&mut self, mut pending: Vec<usize>, mut merge: bool) {
        while let Some(id) = pending.pop() {
            let Some(entry) = self.nodes.get(&id) else {
                continue;
            };
            if matches!(entry.kind, Kind::Relay)
                && (entry.parents.len() <= 1 || entry.children.len() <= 1)
            {
                merge = true;
                pending.extend(self.erase(id, true));
            }
        }
        if !merge || self.relays < 2 {
            return;
        }
        // Equal ancestry groups are interchangeable. Merging them prevents a
        // repeated branching/reconvergence history from retaining relay layers.
        loop {
            let mut groups = BTreeMap::<Vec<usize>, usize>::new();
            let signatures = self.ancestry_signatures();
            let duplicate = self.nodes.iter().find_map(|(id, entry)| {
                if !matches!(entry.kind, Kind::Relay) {
                    return None;
                }
                let key = signatures[id].clone();
                groups.insert(key, *id).map(|keep| (keep, *id))
            });
            let Some((keep, remove)) = duplicate else {
                break;
            };
            let affected = self.merge_relays(keep, remove);
            pending.extend(affected);
            while let Some(id) = pending.pop() {
                let Some(entry) = self.nodes.get(&id) else {
                    continue;
                };
                if matches!(entry.kind, Kind::Relay)
                    && (entry.parents.len() <= 1 || entry.children.len() <= 1)
                {
                    pending.extend(self.erase(id, true));
                }
            }
        }
    }

    fn ancestry_signatures(&self) -> BTreeMap<usize, Vec<usize>> {
        let mut signatures = BTreeMap::<usize, Vec<usize>>::new();
        for (id, entry) in &self.nodes {
            let signature = match &entry.kind {
                Kind::Token(_) => vec![*id],
                Kind::Relay => entry
                    .parents
                    .iter()
                    .flat_map(|parent| signatures[parent].iter().copied())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            };
            signatures.insert(*id, signature);
        }
        signatures
    }

    fn merge_relays(&mut self, keep: usize, remove: usize) -> Vec<usize> {
        let entry = self.nodes.remove(&remove).expect("duplicate relay");
        self.relays -= 1;
        let children = entry.children.clone();
        for parent in &entry.parents {
            self.nodes
                .get_mut(parent)
                .expect("live predecessor")
                .children
                .remove(&remove);
        }
        for child in &children {
            let node = self.nodes.get_mut(child).expect("live descendant");
            node.parents.remove(&remove);
            node.parents.insert(keep);
        }
        let retained = self.nodes.get_mut(&keep).expect("retained relay");
        retained.children.extend(&children);
        retained.cancelled |= entry.cancelled;
        children.into_iter().chain([keep]).collect()
    }

    pub(super) fn cancel(&mut self, id: usize) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let node = self.nodes.get_mut(&id).expect("live cancellation path");
            if node.cancelled {
                continue;
            }
            node.cancelled = true;
            if let Kind::Token(flag) = &node.kind {
                flag.store(true, Ordering::Release);
            }
            pending.extend(&node.children);
        }
    }

    pub(super) fn is_cancelled(&self, id: usize) -> bool {
        self.nodes[&id].cancelled
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> (usize, usize, usize) {
        let tokens = self
            .nodes
            .values()
            .filter(|entry| matches!(entry.kind, Kind::Token(_)))
            .count();
        let relays = self.relays;
        let links = self.nodes.values().map(|entry| entry.children.len()).sum();
        (tokens, relays, links)
    }
}

#[cfg(test)]
#[path = "cancellation_graph_test.rs"]
mod cancellation_graph_test;
