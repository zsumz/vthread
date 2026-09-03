//! Inherited cancellation with live ancestry and bounded generation subscriptions.

use crate::{Error, Result, signal::lock, wait::WaitRegistration};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use vthread_stack::ParkToken;

#[path = "cancellation_graph.rs"]
mod graph;

struct Domain {
    capacity: usize,
    epoch: Arc<AtomicU64>,
    retired: Mutex<Vec<usize>>,
    state: Mutex<State>,
}

const RETIREMENT_BATCH: usize = 64;

#[derive(Default)]
struct State {
    graph: graph::Graph,
    waits: BTreeMap<ParkToken, (usize, WaitRegistration)>,
}

struct Node {
    id: usize,
    cancelled: AtomicBool,
    domain: Arc<Domain>,
}

impl Drop for Node {
    fn drop(&mut self) {
        // Graph entries contain only IDs, flags and inert ancestry signatures.
        // Narrow history splices away; high-degree history becomes a compressed
        // relay, so this drop never recursively destroys ancestor tokens.
        self.domain.retire(self.id);
    }
}

impl Domain {
    fn retire(self: &Arc<Self>, id: usize) {
        // Dead entries remain valid ancestry relays. Retiring in fixed batches
        // leaves at most 63 inert graph entries while avoiding one graph lock
        // and topology rewrite per completed task.
        let mut retired = lock(&self.retired);
        retired.push(id);
        if retired.len() < RETIREMENT_BATCH && Arc::strong_count(self) != 1 {
            return;
        }
        let mut state = lock(&self.state);
        for id in retired.drain(..) {
            state.graph.remove(id);
        }
    }

    #[cfg(test)]
    fn flush_retired(&self) {
        let mut retired = lock(&self.retired);
        let mut state = lock(&self.state);
        for id in retired.drain(..) {
            state.graph.remove(id);
        }
    }

    #[cfg(test)]
    fn pending_retirements(&self) -> usize {
        lock(&self.retired).len()
    }
}

/// A cooperative cancellation request, inherited by child scopes.
#[derive(Clone)]
pub struct CancellationToken(Arc<Node>);

impl CancellationToken {
    pub(crate) fn root(capacity: usize) -> Self {
        Self::insert(
            Arc::new(Domain {
                capacity,
                epoch: Arc::new(AtomicU64::new(1)),
                retired: Mutex::new(Vec::with_capacity(RETIREMENT_BATCH)),
                state: Mutex::default(),
            }),
            &[],
        )
    }

    fn insert(domain: Arc<Domain>, parents: &[usize]) -> Self {
        let mut state = lock(&domain.state);
        let id = state.graph.reserve();
        let node = Arc::new(Node {
            id,
            cancelled: AtomicBool::new(false),
            domain: Arc::clone(&domain),
        });
        state.graph.insert(id, parents, Arc::downgrade(&node));
        drop(state);
        Self(node)
    }

    /// Creates a child token; cancelling a child never cancels its parent.
    pub fn child_token(&self) -> Self {
        Self::insert(Arc::clone(&self.0.domain), &[self.0.id])
    }

    pub(crate) fn child_for_scope(&self, scope: &Self) -> Self {
        assert!(Arc::ptr_eq(&self.0.domain, &scope.0.domain));
        Self::insert(Arc::clone(&self.0.domain), &[self.0.id, scope.0.id])
    }

    /// Requests cancellation and wakes subscribed generations without preempting code.
    pub fn cancel(&self) {
        let selected = {
            let mut state = lock(&self.0.domain.state);
            state.graph.cancel(self.0.id);
            self.0.domain.epoch.fetch_add(1, Ordering::Release);
            state
                .waits
                .iter()
                .filter(|(_, (id, _))| state.graph.is_cancelled(*id))
                .map(|(token, (_, wait))| (*token, wait.clone()))
                .collect::<Vec<_>>()
        };
        // Wake selection can acquire scheduler locks; no domain lock spans it.
        for (token, wait) in selected {
            wait.select_cancelled(token);
        }
    }

    /// Whether this token or an ancestor has requested cancellation.
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn into_cancellation_probe(self) -> (Self, Arc<AtomicU64>) {
        let epoch = Arc::clone(&self.0.domain.epoch);
        (self, epoch)
    }

    pub(crate) fn register(
        &self,
        token: ParkToken,
        wait: WaitRegistration,
    ) -> Result<Subscription> {
        let mut state = lock(&self.0.domain.state);
        if state.waits.len() >= self.0.domain.capacity {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::CancellationSubscriptions,
                limit: self.0.domain.capacity,
            });
        }
        state.waits.insert(token, (self.0.id, wait.clone()));
        let cancelled = self.is_cancelled();
        drop(state);
        if cancelled {
            wait.select_cancelled(token);
        }
        Ok(Subscription {
            node: self.clone(),
            token,
        })
    }

    #[cfg(test)]
    fn graph_snapshot(&self) -> (usize, usize, usize) {
        self.0.domain.flush_retired();
        lock(&self.0.domain.state).graph.snapshot()
    }

    #[cfg(test)]
    fn pending_retirements(&self) -> usize {
        self.0.domain.pending_retirements()
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

pub(crate) struct Subscription {
    node: CancellationToken,
    token: ParkToken,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let removed = lock(&self.node.0.domain.state).waits.remove(&self.token);
        drop(removed);
    }
}

#[cfg(test)]
#[path = "cancellation_test.rs"]
mod cancellation_test;

#[cfg(test)]
#[path = "cancellation_history_test.rs"]
mod cancellation_history_test;

#[cfg(test)]
#[path = "cancellation_race_test.rs"]
mod cancellation_race_test;

#[cfg(test)]
#[path = "cancellation_dense_test.rs"]
mod cancellation_dense_test;
