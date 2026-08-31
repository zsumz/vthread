//! Inherited cooperative cancellation with bounded active-generation subscriptions.

use crate::{Error, Result, signal::lock, wait::WaitRegistration};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use vthread_stack::ParkToken;

struct Domain {
    capacity: usize,
    waits: Mutex<BTreeMap<ParkToken, (Weak<Node>, WaitRegistration)>>,
}

struct Node {
    cancelled: AtomicBool,
    parent: Option<CancellationToken>,
    // Only an owned scope token (a direct child of the runtime root), never a task.
    scope: Option<CancellationToken>,
    domain: Arc<Domain>,
}

/// A cooperative cancellation request, inherited by child scopes.
#[derive(Clone)]
pub struct CancellationToken(Arc<Node>);

impl CancellationToken {
    pub(crate) fn root(capacity: usize) -> Self {
        Self(Arc::new(Node {
            cancelled: AtomicBool::new(false),
            parent: None,
            scope: None,
            domain: Arc::new(Domain {
                capacity,
                waits: Mutex::new(BTreeMap::new()),
            }),
        }))
    }

    /// Creates a child token; cancelling a child never cancels its parent.
    pub fn child_token(&self) -> Self {
        Self(Arc::new(Node {
            cancelled: AtomicBool::new(false),
            parent: Some(self.clone()),
            scope: None,
            domain: Arc::clone(&self.0.domain),
        }))
    }

    pub(crate) fn child_for_scope(&self, scope: &Self) -> Self {
        assert!(Arc::ptr_eq(&self.0.domain, &scope.0.domain));
        Self(Arc::new(Node {
            cancelled: AtomicBool::new(false),
            parent: Some(self.clone()),
            scope: Some(scope.clone()),
            domain: Arc::clone(&self.0.domain),
        }))
    }

    /// Requests cancellation and wakes subscribed generations without preempting code.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let selected = lock(&self.0.domain.waits)
            .iter()
            .filter_map(|(token, (node, wait))| {
                let node = CancellationToken(node.upgrade()?);
                node.is_cancelled().then(|| (*token, wait.clone()))
            })
            .collect::<Vec<_>>();
        for (token, wait) in selected {
            wait.select_cancelled(token);
        }
    }

    /// Whether this token or an ancestor has requested cancellation.
    pub fn is_cancelled(&self) -> bool {
        let mut node = Some(self);
        while let Some(token) = node {
            if token.0.cancelled.load(Ordering::Acquire) {
                return true;
            }
            if token.0.scope.as_ref().is_some_and(Self::is_cancelled) {
                return true;
            }
            node = token.0.parent.as_ref();
        }
        false
    }

    pub(crate) fn register(
        &self,
        token: ParkToken,
        wait: WaitRegistration,
    ) -> Result<Subscription> {
        let mut waits = lock(&self.0.domain.waits);
        if waits.len() >= self.0.domain.capacity {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::CancellationSubscriptions,
                limit: self.0.domain.capacity,
            });
        }
        waits.insert(token, (Arc::downgrade(&self.0), wait.clone()));
        let cancelled = self.is_cancelled();
        drop(waits);
        if cancelled {
            wait.select_cancelled(token);
        }
        Ok(Subscription {
            domain: Arc::clone(&self.0.domain),
            token,
        })
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
    domain: Arc<Domain>,
    token: ParkToken,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        lock(&self.domain.waits).remove(&self.token);
    }
}

#[cfg(test)]
#[path = "cancellation_test.rs"]
mod cancellation_test;
