//! Inherited cancellation with live ancestry and bounded generation subscriptions.

use crate::{Error, Result, id_map::IdMap, signal::lock, wait::WaitRegistration};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use vthread_stack::ParkToken;

#[path = "cancellation_graph.rs"]
mod graph;

struct Domain {
    // Admission bounds live task nodes, and each node may publish one wait.
    waits: Box<[Mutex<IdMap<usize, WaitSlot>>]>,
    epoch: Arc<AtomicU64>,
    retired: Mutex<Vec<usize>>,
    state: Mutex<State>,
}

const RETIREMENT_BATCH: usize = 64;

#[derive(Default)]
struct State {
    graph: graph::Graph,
}

struct Node {
    id: usize,
    cancelled: AtomicBool,
    domain: Arc<Domain>,
}

struct WaitSlot {
    token: ParkToken,
    registration: WaitRegistration,
}

const WAIT_SHARDS: usize = 64;

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
        // and topology rewrite per completed task. The final bounded residual
        // is destroyed with the domain, so retirement does not probe the
        // domain's strong count or rewrite a graph that is about to be freed.
        let mut retired = lock(&self.retired);
        retired.push(id);
        if retired.len() < RETIREMENT_BATCH {
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
        assert!(capacity != 0, "cancellation capacity must be positive");
        let domain = Arc::new(Domain {
            waits: (0..capacity.min(WAIT_SHARDS))
                .map(|_| Mutex::new(IdMap::default()))
                .collect(),
            epoch: Arc::new(AtomicU64::new(1)),
            retired: Mutex::new(Vec::with_capacity(RETIREMENT_BATCH)),
            state: Mutex::default(),
        });
        Self::insert(&domain, &[])
    }

    fn insert(domain: &Arc<Domain>, parents: &[usize]) -> Self {
        let mut state = lock(&domain.state);
        let id = state.graph.reserve();
        let node = Arc::new(Node {
            id,
            cancelled: AtomicBool::new(false),
            domain: Arc::clone(domain),
        });
        let cancelled = state.graph.insert(id, parents, Arc::downgrade(&node));
        node.cancelled.store(cancelled, Ordering::Release);
        drop(state);
        Self(node)
    }

    /// Creates a child token; cancelling a child never cancels its parent.
    pub fn child_token(&self) -> Self {
        Self::insert(&self.0.domain, &[self.0.id])
    }

    pub(crate) fn child_for_scope(&self, scope: &Self) -> Self {
        assert!(Arc::ptr_eq(&self.0.domain, &scope.0.domain));
        Self::insert(&self.0.domain, &[self.0.id, scope.0.id])
    }

    /// Requests cancellation and wakes subscribed generations without preempting code.
    pub fn cancel(&self) {
        let cancelled = {
            let mut state = lock(&self.0.domain.state);
            let cancelled = state.graph.cancel_nodes(self.0.id);
            self.0.domain.epoch.fetch_add(1, Ordering::Release);
            cancelled
        };
        let selected = cancelled
            .into_iter()
            .filter_map(|node| {
                lock(node.domain.waits_for(node.id))
                    .get(&node.id)
                    .map(|wait| (wait.token, wait.registration.clone()))
            })
            .collect::<Vec<_>>();
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

    pub(crate) fn shares_epoch(&self, epoch: &Arc<AtomicU64>) -> bool {
        Arc::ptr_eq(&self.0.domain.epoch, epoch)
    }

    pub(crate) fn register(
        &self,
        token: ParkToken,
        wait: &WaitRegistration,
    ) -> Result<Subscription> {
        let mut waits = lock(self.0.domain.waits_for(self.0.id));
        if waits.contains_key(&self.0.id) {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "task registered concurrent cancellation waits",
            ));
        }
        waits.insert(
            self.0.id,
            WaitSlot {
                token,
                registration: wait.clone(),
            },
        );
        let cancelled = self.is_cancelled();
        drop(waits);
        if cancelled {
            wait.select_cancelled(token);
        }
        Ok(Subscription {
            node: Arc::clone(&self.0),
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
    node: Arc<Node>,
    token: ParkToken,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let mut waits = lock(self.node.domain.waits_for(self.node.id));
        let remove = waits
            .get(&self.node.id)
            .is_some_and(|wait| wait.token == self.token);
        if remove {
            drop(waits.remove(&self.node.id));
        }
    }
}

impl Domain {
    fn waits_for(&self, node: usize) -> &Mutex<IdMap<usize, WaitSlot>> {
        &self.waits[node % self.waits.len()]
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
