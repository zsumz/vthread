//! Non-owning live cancellation edges; callers serialize all graph transitions.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct Entry {
    cancelled: Arc<AtomicBool>,
    parents: BTreeSet<usize>,
    children: BTreeSet<usize>,
}

#[derive(Default)]
pub(super) struct Graph {
    next: usize,
    nodes: BTreeMap<usize, Entry>,
}

impl Graph {
    pub(super) fn insert(&mut self, parents: &[usize], cancelled: Arc<AtomicBool>) -> usize {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("cancellation identity exhausted");
        // Registration and propagation share the caller's lock: a concurrent cancel
        // either visits the new edge or is inherited here, including both owners.
        cancelled.store(
            parents.iter().any(|id| self.is_cancelled(*id)),
            Ordering::Release,
        );
        for parent in parents {
            self.nodes
                .get_mut(parent)
                .expect("live parent")
                .children
                .insert(id);
        }
        self.nodes.insert(
            id,
            Entry {
                cancelled,
                parents: parents.iter().copied().collect(),
                children: BTreeSet::new(),
            },
        );
        id
    }

    pub(super) fn remove(&mut self, id: usize) {
        let entry = self.nodes.remove(&id).expect("live cancellation node");
        for parent in &entry.parents {
            let node = self.nodes.get_mut(parent).expect("live predecessor");
            node.children.remove(&id);
            node.children.extend(&entry.children);
        }
        for child in &entry.children {
            let node = self.nodes.get_mut(child).expect("live descendant");
            node.parents.remove(&id);
            node.parents.extend(&entry.parents);
        }
    }

    pub(super) fn cancel(&mut self, id: usize) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let node = &self.nodes[&id];
            // A previously cancelled node's descendants were already propagated or
            // inherited cancellation at insertion. Each live node is visited once.
            if !node.cancelled.swap(true, Ordering::AcqRel) {
                pending.extend(&node.children);
            }
        }
    }

    pub(super) fn is_cancelled(&self, id: usize) -> bool {
        self.nodes[&id].cancelled.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> (usize, usize) {
        (
            self.nodes.len(),
            self.nodes.values().map(|node| node.children.len()).sum(),
        )
    }
}

#[cfg(test)]
#[path = "cancellation_graph_test.rs"]
mod cancellation_graph_test;
