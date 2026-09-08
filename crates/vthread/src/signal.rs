//! Poison-tolerant internal locks and lost-wakeup-free carrier notifications.

use std::sync::{
    Condvar, Mutex, MutexGuard,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::Instant;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Default)]
pub(crate) struct Signal {
    epoch: AtomicU64,
    waiters: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
    #[cfg(test)]
    before_wait_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    pub(crate) test_progress: TestCarrierProgress,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum TestCarrierPhase {
    Created,
    Drive,
    Receive,
    Tick,
    Idle,
    Waiting,
}

#[cfg(test)]
pub(crate) struct TestCarrierState {
    pub(crate) remote_pending: bool,
    pub(crate) admission_pressure: u32,
    pub(crate) ready: usize,
    pub(crate) incoming: usize,
    pub(crate) pending_task: Option<u64>,
    pub(crate) completions: usize,
    pub(crate) in_flight: Option<u64>,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestCarrierProgress {
    enabled: std::sync::atomic::AtomicBool,
    sequence: AtomicU64,
    drives: AtomicU64,
    phase: AtomicUsize,
    observed_epoch: AtomicU64,
    handled_epoch: AtomicU64,
    remote_pending: std::sync::atomic::AtomicBool,
    admission_pressure: AtomicUsize,
    ready: AtomicUsize,
    incoming: AtomicUsize,
    pending_task: AtomicU64,
    completions: AtomicUsize,
    in_flight: AtomicU64,
}

#[cfg(test)]
impl TestCarrierProgress {
    const NO_EPOCH: u64 = u64::MAX;
    const NO_TASK: u64 = u64::MAX;

    pub(crate) fn enable(&self) {
        self.handled_epoch.store(Self::NO_EPOCH, Ordering::Relaxed);
        self.pending_task.store(Self::NO_TASK, Ordering::Relaxed);
        self.in_flight.store(Self::NO_TASK, Ordering::Relaxed);
        self.enabled.store(true, Ordering::Release);
    }

    pub(crate) fn record_loop(&self, observed: u64, handled: Option<u64>) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.drives.fetch_add(1, Ordering::Relaxed);
        self.observed_epoch.store(observed, Ordering::Relaxed);
        self.handled_epoch
            .store(handled.unwrap_or(Self::NO_EPOCH), Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn record_handled(&self, handled: u64) {
        if self.enabled.load(Ordering::Acquire) {
            self.sequence.fetch_add(1, Ordering::AcqRel);
            self.handled_epoch.store(handled, Ordering::Relaxed);
            self.sequence.fetch_add(1, Ordering::Release);
        }
    }

    pub(crate) fn record_state(&self, phase: TestCarrierPhase, state: TestCarrierState) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.remote_pending
            .store(state.remote_pending, Ordering::Relaxed);
        self.admission_pressure
            .store(state.admission_pressure as usize, Ordering::Relaxed);
        self.ready.store(state.ready, Ordering::Relaxed);
        self.incoming.store(state.incoming, Ordering::Relaxed);
        self.pending_task.store(
            state.pending_task.unwrap_or(Self::NO_TASK),
            Ordering::Relaxed,
        );
        self.completions.store(state.completions, Ordering::Relaxed);
        self.in_flight
            .store(state.in_flight.unwrap_or(Self::NO_TASK), Ordering::Relaxed);
        self.phase.store(phase as usize, Ordering::Release);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn snapshot(&self) -> TestCarrierProgressSnapshot {
        let before = self.sequence.load(Ordering::Acquire);
        let phase = match self.phase.load(Ordering::Acquire) {
            0 => TestCarrierPhase::Created,
            1 => TestCarrierPhase::Drive,
            2 => TestCarrierPhase::Receive,
            3 => TestCarrierPhase::Tick,
            4 => TestCarrierPhase::Idle,
            5 => TestCarrierPhase::Waiting,
            _ => unreachable!("carrier phase"),
        };
        let handled = self.handled_epoch.load(Ordering::Relaxed);
        let pending_task = self.pending_task.load(Ordering::Relaxed);
        let in_flight = self.in_flight.load(Ordering::Relaxed);
        let mut snapshot = TestCarrierProgressSnapshot {
            coherent: false,
            sequence: before,
            drives: self.drives.load(Ordering::Relaxed),
            phase,
            observed_epoch: self.observed_epoch.load(Ordering::Relaxed),
            handled_epoch: (handled != Self::NO_EPOCH).then_some(handled),
            remote_pending: self.remote_pending.load(Ordering::Relaxed),
            admission_pressure: self.admission_pressure.load(Ordering::Relaxed),
            ready: self.ready.load(Ordering::Relaxed),
            incoming: self.incoming.load(Ordering::Relaxed),
            pending_task: (pending_task != Self::NO_TASK).then_some(pending_task),
            completions: self.completions.load(Ordering::Relaxed),
            in_flight: (in_flight != Self::NO_TASK).then_some(in_flight),
        };
        let after = self.sequence.load(Ordering::Acquire);
        snapshot.coherent = before == after && before.is_multiple_of(2);
        snapshot
    }
}

#[cfg(test)]
pub(crate) struct TestCarrierProgressSnapshot {
    coherent: bool,
    sequence: u64,
    drives: u64,
    phase: TestCarrierPhase,
    observed_epoch: u64,
    handled_epoch: Option<u64>,
    remote_pending: bool,
    admission_pressure: usize,
    ready: usize,
    incoming: usize,
    pending_task: Option<u64>,
    completions: usize,
    in_flight: Option<u64>,
}

#[cfg(test)]
impl std::fmt::Debug for TestCarrierProgressSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CarrierProgress")
            .field("coherent", &self.coherent)
            .field("sequence", &self.sequence)
            .field("drives", &self.drives)
            .field("phase", &self.phase)
            .field("observed_epoch", &self.observed_epoch)
            .field("handled_epoch", &self.handled_epoch)
            .field("remote_pending", &self.remote_pending)
            .field("admission_pressure", &self.admission_pressure)
            .field("ready", &self.ready)
            .field("incoming", &self.incoming)
            .field("pending_task", &self.pending_task)
            .field("completions", &self.completions)
            .field("in_flight", &self.in_flight)
            .finish()
    }
}

impl Signal {
    pub(crate) fn version(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(crate) fn notify(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        // With sequential consistency, either this observes the registered
        // waiter or that waiter observes the new epoch before sleeping.
        if self.waiters.load(Ordering::SeqCst) != 0 {
            let _gate = lock(&self.gate);
            self.changed.notify_all();
        }
    }

    pub(crate) fn notify_if_waiting(&self) {
        if self.waiters.load(Ordering::SeqCst) != 0 {
            // Predicate-backed waits do not need an epoch change: the gate
            // handoff makes the published work visible without a lost wake,
            // and every inbox has exactly one owner carrier to notify.
            let _gate = lock(&self.gate);
            self.changed.notify_one();
        }
    }

    pub(crate) fn wait(&self, observed: u64, deadline: Option<Instant>) {
        self.wait_while(observed, deadline, || false);
    }

    pub(crate) fn wait_while(
        &self,
        observed: u64,
        deadline: Option<Instant>,
        mut ready: impl FnMut() -> bool,
    ) {
        #[cfg(test)]
        let hook = lock(&self.before_wait_hook).take();
        #[cfg(test)]
        if let Some(hook) = hook {
            hook();
        }
        let mut gate = lock(&self.gate);
        self.waiters.fetch_add(1, Ordering::SeqCst);
        while self.epoch.load(Ordering::SeqCst) == observed && !ready() {
            gate = if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                #[cfg(feature = "handoff-profiling")]
                let _native = crate::handoff_span::Span::new(
                    crate::handoff_profile::HandoffStage::NativeWait,
                );
                let (guard, _) = self
                    .changed
                    .wait_timeout(gate, remaining)
                    .unwrap_or_else(|poison| poison.into_inner());
                guard
            } else {
                #[cfg(feature = "handoff-profiling")]
                let _native = crate::handoff_span::Span::new(
                    crate::handoff_profile::HandoffStage::NativeWait,
                );
                self.changed
                    .wait(gate)
                    .unwrap_or_else(|poison| poison.into_inner())
            };
        }
        self.waiters.fetch_sub(1, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiters.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn before_wait(&self, hook: impl FnOnce() + Send + 'static) {
        *lock(&self.before_wait_hook) = Some(Box::new(hook));
    }
}

#[cfg(test)]
#[path = "signal_test.rs"]
mod signal_test;
