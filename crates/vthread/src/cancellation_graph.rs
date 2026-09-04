//! Compressed cancellation graph; callers serialize every transition.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Weak, atomic::Ordering},
};

use super::Node;
use crate::id_map::IdMap;

#[path = "cancellation_id_set.rs"]
mod id_set;
use id_set::IdSet;

#[path = "cancellation_parent_set.rs"]
mod parent_set;
use parent_set::ParentSet;

#[path = "cancellation_signature.rs"]
mod signature;
use signature::{Candidate, Signature};

#[path = "cancellation_graph_normalize.rs"]
mod normalize;
#[path = "cancellation_graph_rewrite.rs"]
mod rewrite;
use normalize::RelayWork;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Token,
    Relay,
}

struct Entry {
    kind: Kind,
    flag: Option<Weak<Node>>,
    cancelled: bool,
    parents: ParentSet,
    children: IdSet,
    signature: Signature,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WorkSnapshot {
    pub(super) union_items: usize,
    pub(super) equality_nodes: usize,
    pub(super) allocated_nodes: usize,
    pub(super) candidate_checks: usize,
    pub(super) relay_checks: usize,
    pub(super) topology_rebuilds: usize,
}

#[derive(Default)]
pub(super) struct Graph {
    next: usize,
    relays: usize,
    nodes: IdMap<usize, Entry>,
    relay_index: BTreeMap<Candidate, BTreeSet<usize>>,
    #[cfg(test)]
    work: WorkSnapshot,
}

impl Graph {
    pub(super) fn reserve(&mut self) -> usize {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("cancellation identity exhausted");
        id
    }

    pub(super) fn insert(&mut self, id: usize, parents: &[usize], flag: Weak<Node>) -> bool {
        assert_eq!(id + 1, self.next, "cancellation identity not reserved");
        let (parents, cancelled) = match parents {
            [] => (ParentSet::default(), false),
            [parent] => {
                let entry = self.nodes.get_mut(parent).expect("live parent");
                entry.children.insert(id);
                (ParentSet::One(*parent), entry.cancelled)
            }
            parents => {
                let parents = parents.iter().copied().collect::<ParentSet>();
                let mut cancelled = false;
                for parent in &parents {
                    let entry = self.nodes.get_mut(parent).expect("live parent");
                    cancelled |= entry.cancelled;
                    entry.children.insert(id);
                }
                (parents, cancelled)
            }
        };
        self.nodes.insert(
            id,
            Entry {
                kind: Kind::Token,
                flag: Some(flag),
                cancelled,
                parents,
                children: IdSet::default(),
                signature: Signature::singleton(id),
            },
        );
        cancelled
    }

    #[cfg(test)]
    fn insert_inert(&mut self, parents: &[usize]) -> usize {
        let id = self.reserve();
        let _ = self.insert(id, parents, Weak::new());
        id
    }

    pub(super) fn remove(&mut self, id: usize) {
        let direct_parent = {
            let entry = &self.nodes[&id];
            if entry.children.is_empty()
                && let ParentSet::One(parent) = entry.parents
                && self.nodes[&parent].kind == Kind::Token
            {
                Some(parent)
            } else {
                None
            }
        };
        if let Some(parent) = direct_parent {
            let entry = self.nodes.remove(&id).expect("live cancellation leaf");
            assert_eq!(entry.kind, Kind::Token, "token removed twice");
            self.nodes
                .get_mut(&parent)
                .expect("live predecessor")
                .children
                .remove(&id);
            return;
        }
        let direct_leaf = {
            let entry = &self.nodes[&id];
            entry.children.is_empty()
                && entry
                    .parents
                    .iter()
                    .all(|parent| self.nodes[parent].kind == Kind::Token)
        };
        if direct_leaf {
            let entry = self.nodes.remove(&id).expect("live cancellation leaf");
            assert_eq!(entry.kind, Kind::Token, "token removed twice");
            for parent in entry.parents {
                self.nodes
                    .get_mut(&parent)
                    .expect("live predecessor")
                    .children
                    .remove(&id);
            }
            return;
        }
        let (parents, children, inherited, relay) = {
            let entry = self.nodes.get(&id).expect("live cancellation node");
            assert_eq!(entry.kind, Kind::Token, "token removed twice");
            (
                entry.parents.clone(),
                entry.children.clone(),
                entry
                    .parents
                    .iter()
                    .any(|parent| self.nodes[parent].cancelled),
                entry.parents.len() > 1 && entry.children.len() > 1,
            )
        };
        if relay {
            let signature = self.signature_for(&parents);
            let entry = self.nodes.get_mut(&id).expect("live cancellation node");
            entry.kind = Kind::Relay;
            entry.flag = None;
            entry.cancelled = inherited;
            entry.signature = signature;
            self.relays += 1;
            self.index_relay(id);
            self.normalize(
                std::iter::once(RelayWork::known(id))
                    .chain(children.into_iter().map(RelayWork::dirty)),
            );
        } else {
            let affected = self.erase(id, true, true);
            self.normalize(affected);
        }
    }

    pub(super) fn cancel_nodes(&mut self, id: usize) -> Vec<std::sync::Arc<Node>> {
        let mut pending = vec![id];
        let mut changed_relays = Vec::new();
        let mut cancelled = Vec::new();
        while let Some(id) = pending.pop() {
            let children = {
                let node = self.nodes.get_mut(&id).expect("live cancellation path");
                if node.cancelled {
                    continue;
                }
                node.cancelled = true;
                if let Some(flag) = &node.flag {
                    if let Some(node) = flag.upgrade() {
                        node.cancelled.store(true, Ordering::Release);
                        cancelled.push(node);
                    }
                } else {
                    changed_relays.push(id);
                }
                node.children.iter().copied().collect::<Vec<_>>()
            };
            pending.extend(children);
        }
        self.normalize(changed_relays.into_iter().map(RelayWork::known));
        cancelled
    }

    #[cfg(test)]
    pub(super) fn cancel(&mut self, id: usize) {
        drop(self.cancel_nodes(id));
    }

    #[cfg(test)]
    pub(super) fn is_cancelled(&self, id: usize) -> bool {
        self.nodes[&id].cancelled
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> (usize, usize, usize) {
        let tokens = self
            .nodes
            .values()
            .filter(|entry| entry.kind == Kind::Token)
            .count();
        let links = self.nodes.values().map(|entry| entry.children.len()).sum();
        (tokens, self.relays, links)
    }

    #[cfg(test)]
    pub(super) fn work_snapshot(&self) -> WorkSnapshot {
        self.work
    }

    #[cfg(test)]
    pub(super) fn reset_work(&mut self) {
        self.work = WorkSnapshot::default();
    }

    #[cfg(test)]
    fn record_signature_work(&mut self, work: signature::Work) {
        self.work.union_items += work.union_items;
        self.work.equality_nodes += work.equality_nodes;
        self.work.allocated_nodes += work.allocated_nodes;
    }
}

#[cfg(test)]
#[path = "cancellation_graph_test.rs"]
mod cancellation_graph_test;

#[cfg(test)]
#[path = "cancellation_graph_differential_test.rs"]
mod cancellation_graph_differential_test;
