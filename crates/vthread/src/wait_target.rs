//! Stable target metadata published before an atomic wait generation becomes active.

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use vthread_stack::ParkToken;

use crate::{
    TaskId,
    signal::lock,
    task_slab::TaskKey,
    wait::{
        WaitHub,
        wait_state::{Phase, WaitWord},
    },
};

pub(crate) struct WaitInner {
    pub(super) id: u64,
    state: AtomicU64,
    task: AtomicU64,
    route: AtomicUsize,
    primary_hub: OnceLock<Arc<WaitHub>>,
    fallback_hub: Mutex<Option<Arc<WaitHub>>>,
}

impl WaitInner {
    pub(super) fn new(id: u64) -> Self {
        Self {
            id,
            state: AtomicU64::new(WaitWord::initial().raw()),
            task: AtomicU64::new(0),
            route: AtomicUsize::new(0),
            primary_hub: OnceLock::new(),
            fallback_hub: Mutex::new(None),
        }
    }

    #[inline]
    pub(super) fn load(&self) -> WaitWord {
        WaitWord::from_raw(self.state.load(Ordering::Acquire))
    }

    #[inline]
    pub(super) fn compare_exchange(
        &self,
        current: WaitWord,
        next: WaitWord,
    ) -> std::result::Result<(), WaitWord> {
        self.state
            .compare_exchange(
                current.raw(),
                next.raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(WaitWord::from_raw)
    }

    #[inline]
    pub(super) fn store(&self, word: WaitWord) {
        self.state.store(word.raw(), Ordering::Release);
    }

    #[inline]
    pub(super) fn publish_claim(&self, claimed: WaitWord) {
        // Claims are write-exclusive: selectors reject them, while close and
        // permit mutation wait for the selected phase.
        #[cfg(debug_assertions)]
        assert!(self.load() == claimed);
        self.store(claimed.publish_claim());
    }

    pub(super) fn bind_target(&self, task: TaskId, route: TaskKey, hub: &Arc<WaitHub>) -> bool {
        self.task.store(task.get(), Ordering::Relaxed);
        self.route.store(route.encoded(), Ordering::Relaxed);
        if self.primary_hub.get().is_none() {
            let _ = self.primary_hub.set(Arc::clone(hub));
        }
        let primary = self
            .primary_hub
            .get()
            .expect("initialized primary wait hub");
        if Arc::ptr_eq(primary, hub) {
            return false;
        }
        let mut fallback = lock(&self.fallback_hub);
        if fallback
            .as_ref()
            .is_none_or(|cached| !Arc::ptr_eq(cached, hub))
        {
            *fallback = Some(Arc::clone(hub));
        }
        true
    }

    pub(super) fn cached_target(
        &self,
        task: TaskId,
        route: TaskKey,
        hub: &Arc<WaitHub>,
    ) -> Option<bool> {
        if self.task.load(Ordering::Relaxed) != task.get()
            || self.route.load(Ordering::Relaxed) != route.encoded()
        {
            return None;
        }
        let primary = self.primary_hub.get()?;
        if Arc::ptr_eq(primary, hub) {
            return Some(false);
        }
        lock(&self.fallback_hub)
            .as_ref()
            .is_some_and(|fallback| Arc::ptr_eq(fallback, hub))
            .then_some(true)
    }

    pub(super) fn with_target<R>(
        &self,
        word: WaitWord,
        use_target: impl FnOnce(TaskId, TaskKey, &Arc<WaitHub>) -> R,
    ) -> R {
        let task = TaskId::new(self.task.load(Ordering::Relaxed));
        let route = TaskKey::from_encoded(self.route.load(Ordering::Relaxed));
        if word.uses_fallback_hub() {
            let fallback = lock(&self.fallback_hub);
            return use_target(
                task,
                route,
                fallback.as_ref().expect("published fallback wait hub"),
            );
        }
        use_target(
            task,
            route,
            self.primary_hub.get().expect("published primary wait hub"),
        )
    }

    pub(super) fn clone_hub(&self, word: WaitWord) -> Arc<WaitHub> {
        self.with_target(word, |_, _, hub| Arc::clone(hub))
    }

    pub(super) fn retire(&self, token: ParkToken) -> Option<Arc<WaitHub>> {
        if token.wait() != self.id {
            return None;
        }
        loop {
            let word = self.load();
            if word.generation() != token.generation() || word.phase() == Phase::Idle {
                return None;
            }
            if word.is_claimed() || word.phase() == Phase::Binding {
                std::hint::spin_loop();
                continue;
            }
            let hub = self.clone_hub(word);
            if self.compare_exchange(word, word.retire()).is_ok() {
                return Some(hub);
            }
        }
    }
}

#[cfg(test)]
#[path = "wait_target_test.rs"]
mod wait_target_test;
