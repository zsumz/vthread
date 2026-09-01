//! Incremental relay compaction and exact ancestry indexing.

use super::{Graph, Kind, Signature};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use super::signature::Work as SignatureWork;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SignatureState {
    Known,
    Dirty,
}

#[derive(Clone, Copy)]
pub(super) struct RelayWork {
    id: usize,
    signature: SignatureState,
}

impl RelayWork {
    pub(super) fn known(id: usize) -> Self {
        Self {
            id,
            signature: SignatureState::Known,
        }
    }

    pub(super) fn dirty(id: usize) -> Self {
        Self {
            id,
            signature: SignatureState::Dirty,
        }
    }
}

impl Graph {
    pub(super) fn normalize<I>(&mut self, initial: I)
    where
        I: IntoIterator<Item = RelayWork>,
    {
        let mut pending = BTreeMap::new();
        for work in initial {
            enqueue(&mut pending, work);
        }
        while let Some((id, signature)) = pending.pop_first() {
            let Some(entry) = self.nodes.get(&id) else {
                continue;
            };
            if entry.kind != Kind::Relay {
                continue;
            }
            if entry.parents.len() <= 1 || entry.children.len() <= 1 {
                let affected = self.erase(id, true, signature == SignatureState::Dirty);
                for work in affected {
                    enqueue(&mut pending, work);
                }
                continue;
            }
            if signature == SignatureState::Dirty {
                self.refresh_signature(id, &mut pending);
            }
            #[cfg(test)]
            {
                self.work.relay_checks += 1;
            }
            let Some(other) = self.equivalent_relay(id) else {
                continue;
            };
            let (keep, remove) = if id < other { (id, other) } else { (other, id) };
            for work in self.merge_relays(keep, remove) {
                enqueue(&mut pending, work);
            }
        }
    }

    fn refresh_signature(&mut self, id: usize, pending: &mut BTreeMap<usize, SignatureState>) {
        let parents = self.nodes[&id].parents.clone();
        let signature = self.signature_for(&parents);
        let previous = self.nodes[&id].signature.clone();
        #[cfg(test)]
        {
            self.work.topology_rebuilds += 1;
        }
        if self.signatures_equal(&previous, &signature) {
            return;
        }
        self.unindex_relay(id);
        let children = {
            let entry = self.nodes.get_mut(&id).expect("live relay");
            entry.signature = signature;
            entry.children.clone()
        };
        self.index_relay(id);
        for child in children {
            enqueue(pending, RelayWork::dirty(child));
        }
    }

    pub(super) fn signature_for(&mut self, parents: &BTreeSet<usize>) -> Signature {
        let mut signature = Signature::default();
        for parent in parents {
            let next = self.nodes[parent].signature.clone();
            #[cfg(test)]
            {
                let (merged, work) = signature.union_counted(&next);
                self.record_signature_work(work);
                signature = merged;
            }
            #[cfg(not(test))]
            {
                signature = signature.union(&next);
            }
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

    pub(super) fn unindex_relay(&mut self, id: usize) {
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
        let mut matched = None;
        #[cfg(test)]
        let mut comparison_work = SignatureWork::default();
        #[cfg(test)]
        let mut candidate_checks = 0;
        if let Some(ids) = self.relay_index.get(&candidate) {
            for other in ids {
                if *other == id {
                    continue;
                }
                #[cfg(test)]
                {
                    candidate_checks += 1;
                }
                let other_entry = self.nodes.get(other).expect("indexed relay");
                if other_entry.cancelled != cancelled {
                    continue;
                }
                #[cfg(test)]
                let same = {
                    let (same, work) = signature.same_set_counted(&other_entry.signature);
                    comparison_work.union_items += work.union_items;
                    comparison_work.equality_nodes += work.equality_nodes;
                    comparison_work.allocated_nodes += work.allocated_nodes;
                    same
                };
                #[cfg(not(test))]
                let same = signature.same_set(&other_entry.signature);
                if same {
                    matched = Some(*other);
                    break;
                }
            }
        }
        #[cfg(test)]
        {
            self.work.candidate_checks += candidate_checks;
            self.record_signature_work(comparison_work);
        }
        matched
    }

    fn signatures_equal(&mut self, left: &Signature, right: &Signature) -> bool {
        #[cfg(test)]
        {
            let (same, work) = left.same_set_counted(right);
            self.record_signature_work(work);
            same
        }
        #[cfg(not(test))]
        {
            left.same_set(right)
        }
    }
}

fn enqueue(pending: &mut BTreeMap<usize, SignatureState>, work: RelayWork) {
    pending
        .entry(work.id)
        .and_modify(|state| {
            if work.signature == SignatureState::Dirty {
                *state = SignatureState::Dirty;
            }
        })
        .or_insert(work.signature);
}

#[cfg(test)]
#[path = "cancellation_graph_normalize_test.rs"]
mod cancellation_graph_normalize_test;
