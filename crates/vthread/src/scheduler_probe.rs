//! Opt-in owner-only counters for admission and idle-pacing attribution.

/// Cumulative activity for one carrier, available with `scheduler-profiling`.
///
/// Counters are updated only by the owner and copied through its existing snapshot
/// publication. They add no clock reads, shared counter atomics, or allocations.
/// Active snapshots can lag; shutdown publishes final totals including startup,
/// warm-up, measured work and shutdown. Instrumented timings are not headline data.
/// A wait call is not proof of a condition-variable syscall or an OS deschedule.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::default::Default,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct SchedulerProfile {
    receive_calls: u64,
    window_deferrals: u64,
    remote_packets: u64,
    batch_histogram: [u64; 8],
    idle_entries: u64,
    idle_dispatches: u64,
    maximum_idle_dispatches: u64,
    empty_idle_entries: u64,
    last_idle_mounts: u64,
    early_work_returns: u64,
    poll_episodes: u64,
    poll_probes: u64,
    poll_hits: u64,
    wait_calls: u64,
    timed_wait_calls: u64,
    returns_without_task_work: u64,
}

impl SchedulerProfile {
    pub(crate) fn record_receive(&mut self, drained: Option<usize>) {
        self.receive_calls += 1;
        if let Some(count) = drained {
            self.remote_packets += count as u64;
            let bucket = (usize::BITS - count.leading_zeros()).min(7) as usize;
            self.batch_histogram[bucket] += 1;
        } else {
            self.window_deferrals += 1;
        }
    }

    pub(crate) fn record_idle(&mut self, mounts: u64) {
        let dispatches = mounts - self.last_idle_mounts;
        self.last_idle_mounts = mounts;
        self.idle_entries += 1;
        self.idle_dispatches += dispatches;
        self.maximum_idle_dispatches = self.maximum_idle_dispatches.max(dispatches);
        self.empty_idle_entries += u64::from(dispatches == 0);
    }

    pub(crate) fn record_early_work(&mut self) {
        self.early_work_returns += 1;
    }

    pub(crate) fn record_poll(&mut self, probes: usize, hit: bool) {
        self.poll_episodes += 1;
        self.poll_probes += probes as u64;
        self.poll_hits += u64::from(hit);
    }

    pub(crate) fn record_wait(&mut self, timed: bool) {
        self.wait_calls += 1;
        self.timed_wait_calls += u64::from(timed);
    }

    pub(crate) fn record_wait_return(&mut self, has_task_work: bool) {
        self.returns_without_task_work += u64::from(!has_task_work);
    }

    /// Remote receive invocations, including empty drains and full-window deferrals.
    pub fn receive_calls(self) -> u64 {
        self.receive_calls
    }

    /// Invocations that deferred remote materialization to preserve the ready window.
    pub fn window_deferrals(self) -> u64 {
        self.window_deferrals
    }

    /// Accepted remote packets drained, including any later materialization failure.
    pub fn remote_packets(self) -> u64 {
        self.remote_packets
    }

    /// Drain-size bins: 0, 1, 2-3, 4-7, 8-15, 16-31, 32-63, and 64 or more.
    /// Full-window deferrals are separate and are not counted as empty drains.
    pub fn batch_histogram(self) -> [u64; 8] {
        self.batch_histogram
    }

    /// Calls to the idle driver, whether they return early, poll or enter a wait.
    pub fn idle_entries(self) -> u64 {
        self.idle_entries
    }

    /// Dispatches preceding idle entries, excluding work after the final idle entry.
    pub fn idle_dispatches(self) -> u64 {
        self.idle_dispatches
    }

    /// Largest number of dispatches between consecutive idle entries (initially zero).
    pub fn maximum_idle_dispatches(self) -> u64 {
        self.maximum_idle_dispatches
    }

    /// Idle entries with no dispatch since the preceding entry (or carrier start).
    pub fn empty_idle_entries(self) -> u64 {
        self.empty_idle_entries
    }

    /// Idle entries that found task work before polling or waiting.
    pub fn early_work_returns(self) -> u64 {
        self.early_work_returns
    }

    /// Bounded busy-poll episodes; timed waits and single-carrier idling skip these.
    pub fn poll_episodes(self) -> u64 {
        self.poll_episodes
    }

    /// Predicate probes across all busy-poll episodes, counted once at each exit.
    pub fn poll_probes(self) -> u64 {
        self.poll_probes
    }

    /// Poll episodes that observed task work or a changed control epoch.
    pub fn poll_hits(self) -> u64 {
        self.poll_hits
    }

    /// Calls into the predicate-backed wait, which may return without a syscall.
    pub fn wait_calls(self) -> u64 {
        self.wait_calls
    }

    /// Wait calls with a timer deadline, including deadlines already due.
    pub fn timed_wait_calls(self) -> u64 {
        self.timed_wait_calls
    }

    /// Wait returns with no start/wake work observed. Timers and control may be due;
    /// this does not count spurious kernel wakeups or prove a native sleep occurred.
    pub fn returns_without_task_work(self) -> u64 {
        self.returns_without_task_work
    }
}

#[cfg(test)]
#[path = "scheduler_probe_test.rs"]
mod scheduler_probe_test;
