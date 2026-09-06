//! Bounded forced handoffs with explicit measured-owner and gating evidence.

use crate::{mutex_handoff_linux as linux, mutex_handoff_tasks as tasks};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Local,
    RemoteActive,
    RemoteSleepObserved,
}
impl Mode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::RemoteActive => "remote-active",
            Self::RemoteSleepObserved => "remote-sleep-observed",
        }
    }
    pub(crate) fn workers(self) -> usize {
        if self == Self::Local { 1 } else { 2 }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Config {
    pub mode: Mode,
    pub iterations: usize,
    pub pin: bool,
}

pub(crate) struct Report {
    pub sender_tid: usize,
    pub recipient_tid: usize,
    pub sender_carrier: usize,
    pub recipient_carrier: usize,
    pub recipient_parks: u64,
    pub source_yields: u64,
    pub keeper_yields: u64,
    pub queued_handoffs: usize,
    pub sleep_observations: usize,
    pub elapsed: Duration,
    pub samples: Vec<u64>,
    sorted: Vec<u64>,
}
impl Report {
    pub(crate) fn percentile(&self, numerator: usize, denominator: usize) -> u64 {
        self.sorted[(self.sorted.len() * numerator)
            .div_ceil(denominator)
            .saturating_sub(1)]
    }
    pub(crate) fn maximum(&self) -> u64 {
        *self.sorted.last().expect("nonempty handoffs")
    }
}

pub(crate) fn run(config: Config) -> vthread::Result<Report> {
    if !(1..=100_000).contains(&config.iterations) {
        return Err(linux::error("invalid bounded work count"));
    }
    let runtime = vthread::Runtime::builder()
        .carriers(config.mode.workers())
        .max_vthreads(8)
        .carrier_queue_capacity(8)
        .io_capacity(1)
        .blocking_threads(1)
        .blocking_capacity(1)
        .stack_size(64 * 1024)
        .stack_cache_capacity(8)
        .build()?;
    if config.pin {
        linux::pin(config.mode.workers())?;
    }
    let (park, wake) = vthread::parking::park_pair();
    let state = Arc::new(tasks::Shared::new(config, wake));
    let start = Instant::now();
    let mut report = runtime.run_scope(|scope| {
        let sending = Arc::clone(&state);
        let mut sender = scope.spawn("mutex-source", move || tasks::sender(&sending))?;
        let receiving = Arc::clone(&state);
        let mut recipient = scope.spawn("mutex-recipient", move || {
            tasks::recipient(&receiving, park)
        })?;
        // Both results are retained before propagating either task's failure.
        let sent = sender.join();
        let received = recipient.join();
        if !sent.as_ref().is_ok_and(|result| result.is_ok())
            || !received.as_ref().is_ok_and(|result| result.is_ok())
        {
            // Peer stop is secondary evidence, not a replacement for the
            // original bad sample, panic, or failed acquisition.
            return Err(linux::error(format!(
                "source outcome: {:?}; recipient outcome: {:?}",
                sent.as_ref().map(|result| result.as_ref().map(|_| ())),
                received.as_ref().map(|result| result.as_ref().map(|_| ()))
            )));
        }
        let (sender_tid, queued_handoffs, sleep_observations) = sent??;
        let (recipient_tid, samples, keeper_yields) = received??;
        let snapshot = runtime.snapshot();
        let source = snapshot
            .tasks()
            .iter()
            .find(|task| task.name() == "mutex-source")
            .ok_or_else(|| linux::error("missing source diagnostics"))?;
        let recipient = snapshot
            .tasks()
            .iter()
            .find(|task| task.name() == "mutex-recipient")
            .ok_or_else(|| linux::error("missing recipient diagnostics"))?;
        check_owners(config.mode, sender_tid, recipient_tid)?;
        if (source.carrier() == recipient.carrier()) != (config.mode == Mode::Local)
            || recipient.parks() < config.iterations as u64
            || samples.len() != config.iterations
            || queued_handoffs != config.iterations
            || state.mutex.waiting() != 0
        {
            return Err(linux::error(
                "forced handoff accounting or topology mismatch",
            ));
        }
        Ok(Report {
            sender_tid,
            recipient_tid,
            sender_carrier: source.carrier().index(),
            recipient_carrier: recipient.carrier().index(),
            recipient_parks: recipient.parks(),
            source_yields: source.yields(),
            keeper_yields,
            queued_handoffs,
            sleep_observations,
            elapsed: start.elapsed(),
            samples,
            sorted: Vec::new(),
        })
    })?;
    runtime.shutdown()?;
    let final_state = runtime.snapshot();
    if final_state.active() != 0 || !final_state.failures().is_empty() {
        return Err(linux::error("mutex fixture did not drain cleanly"));
    }
    report.sorted.clone_from(&report.samples);
    report.sorted.sort_unstable();
    #[cfg(feature = "handoff-profiling")]
    for carrier in final_state.carriers() {
        use std::io::Write;
        let profile = carrier.handoff_profile();
        writeln!(
            std::io::stdout(),
            "phase=diagnostic carrier={:?} local_routes={} shared_routes={} native_wait_calls={}",
            carrier.id(),
            profile.local_publications(),
            profile.remote_publications(),
            profile
                .duration(vthread::diagnostics::HandoffStage::NativeWait)
                .count()
        )?;
    }
    Ok(report)
}

pub(crate) fn check_owners(mode: Mode, sender: usize, recipient: usize) -> vthread::Result<()> {
    if sender == 0 || recipient == 0 || (sender == recipient) != (mode == Mode::Local) {
        return Err(linux::error(
            "actual endpoint owners do not match the requested mechanism",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "mutex_handoff_test.rs"]
mod mutex_handoff_test;
