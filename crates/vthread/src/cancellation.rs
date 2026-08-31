//! Inherited cancellation with live ancestry and bounded generation subscriptions.

use crate::{Error, Result, signal::lock, wait::WaitRegistration};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use vthread_stack::ParkToken;

#[path = "cancellation_graph.rs"]
mod graph;

struct Domain {
    capacity: usize,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    graph: graph::Graph,
    waits: BTreeMap<ParkToken, (usize, WaitRegistration)>,
}

struct Node {
    id: usize,
    cancelled: Arc<AtomicBool>,
    domain: Arc<Domain>,
}

impl Drop for Node {
    fn drop(&mut self) {
        // Graph entries contain only IDs and flags, never tokens or user values.
        // Splicing live neighbors preserves retained ancestors without retaining
        // this historical node or recursively destroying ancestor tokens.
        lock(&self.domain.state).graph.remove(self.id);
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
                state: Mutex::default(),
            }),
            &[],
        )
    }

    fn insert(domain: Arc<Domain>, parents: &[usize]) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let id = lock(&domain.state)
            .graph
            .insert(parents, Arc::clone(&cancelled));
        Self(Arc::new(Node {
            id,
            cancelled,
            domain,
        }))
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
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
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
    fn graph_snapshot(&self) -> (usize, usize) {
        lock(&self.0.domain.state).graph.snapshot()
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
