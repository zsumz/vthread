//! Final-only activity evidence; never observe or reset counters between rounds.

use crate::config::Config;
use std::io::Write;
use vthread::diagnostics::{CarrierStatus, RuntimeSnapshot, ShutdownPhase};

pub(crate) fn report(
    output: &mut impl Write,
    snapshot: &RuntimeSnapshot,
    config: &Config,
) -> Result<(), String> {
    let expected = config
        .samples
        .checked_add(1)
        .and_then(|rounds| rounds.checked_mul(config.tasks))
        .and_then(|tasks| u64::try_from(tasks).ok())
        .ok_or("scheduler profile task count overflow")?;
    validate(snapshot, expected)?;
    #[cfg(feature = "handoff-profiling")]
    let transfers = channel_transfers(config)?;
    writeln!(
        output,
        "engine=vthread phase=scheduler-profile scope=whole-runtime headline=false rounds={} expected_tasks={} batch_bins=0,1,2-3,4-7,8-15,16-31,32-63,64+",
        config.samples + 1,
        expected,
    )
    .map_err(|error| error.to_string())?;
    for carrier in snapshot.carriers() {
        let profile = carrier.scheduler_profile();
        writeln!(
            output,
            "engine=vthread phase=scheduler-admission carrier={} receive_calls={} window_deferrals={} remote_packets={} batch_histogram={:?}",
            carrier.id().index(),
            profile.receive_calls(),
            profile.window_deferrals(),
            profile.remote_packets(),
            profile.batch_histogram(),
        )
        .map_err(|error| error.to_string())?;
        writeln!(
            output,
            "engine=vthread phase=scheduler-idle carrier={} mounts={} idle_entries={} idle_dispatches={} maximum_idle_dispatches={} empty_idle_entries={} early_work_returns={} poll_episodes={} poll_probes={} poll_hits={} wait_calls={} timed_wait_calls={} returns_without_task_work={}",
            carrier.id().index(),
            carrier.stats().mounts(),
            profile.idle_entries(),
            profile.idle_dispatches(),
            profile.maximum_idle_dispatches(),
            profile.empty_idle_entries(),
            profile.early_work_returns(),
            profile.poll_episodes(),
            profile.poll_probes(),
            profile.poll_hits(),
            profile.wait_calls(),
            profile.timed_wait_calls(),
            profile.returns_without_task_work(),
        )
        .map_err(|error| error.to_string())?;
    }
    #[cfg(feature = "handoff-profiling")]
    crate::handoff_profile::report(output, snapshot, transfers)?;
    Ok(())
}

#[cfg(feature = "handoff-profiling")]
fn channel_transfers(config: &Config) -> Result<Option<u64>, String> {
    let crate::config::Scenario::ChannelMpmc { per_task, .. } = config.scenario else {
        return Ok(None);
    };
    config
        .samples
        .checked_add(1)
        .and_then(|rounds| rounds.checked_mul(config.tasks / 2))
        .and_then(|calls| calls.checked_mul(per_task))
        .and_then(|calls| u64::try_from(calls).ok())
        .map(Some)
        .ok_or_else(|| "handoff profile channel transfer count overflow".into())
}

fn validate(snapshot: &RuntimeSnapshot, expected: u64) -> Result<(), String> {
    if snapshot.shutdown_phase() != ShutdownPhase::Complete || snapshot.active() != 0 {
        return Err("scheduler profile requires completed shutdown and no active tasks".into());
    }
    let mut packets = 0;
    for carrier in snapshot.carriers() {
        let profile = carrier.scheduler_profile();
        if carrier.status() != CarrierStatus::Stopped
            || profile.receive_calls()
                != profile.window_deferrals() + profile.batch_histogram().iter().sum::<u64>()
            || profile.idle_entries()
                != profile.early_work_returns() + profile.poll_hits() + profile.wait_calls()
            || profile.poll_hits() > profile.poll_episodes()
            || profile.poll_probes() < profile.poll_episodes()
            || profile.timed_wait_calls() > profile.wait_calls()
            || profile.returns_without_task_work() > profile.wait_calls()
            || profile.empty_idle_entries() > profile.idle_entries()
            || profile.maximum_idle_dispatches() > profile.idle_dispatches()
            || profile.idle_dispatches() > carrier.stats().mounts()
        {
            return Err(format!(
                "inconsistent scheduler profile for {:?}",
                carrier.id()
            ));
        }
        packets += profile.remote_packets();
    }
    if packets != expected || snapshot.stats().completed() != expected {
        return Err(format!(
            "scheduler profile expected {expected} tasks, observed {packets} remote packets and {} completions",
            snapshot.stats().completed(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "scheduler_profile_test.rs"]
mod scheduler_profile_test;
