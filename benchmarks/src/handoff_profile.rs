//! Final-only handoff evidence; elapsed regions overlap and are never CPU claims.

use std::io::Write;
use vthread::diagnostics::{
    CarrierStatus, ChannelDirection, HANDOFF_DURATION_BOUNDS_NS, HandoffProfile, HandoffStage,
    RuntimeSnapshot, ShutdownPhase,
};

pub(crate) fn report(
    output: &mut impl Write,
    snapshot: &RuntimeSnapshot,
    expected_transfers: Option<u64>,
) -> Result<(), String> {
    validate(snapshot, expected_transfers)?;
    writeln!(output,
        "engine=vthread phase=handoff-profile scope=whole-runtime headline=false owner_only=true native_callers=false regions_overlap=true clock_overhead_subtracted=false bounds_ns={HANDOFF_DURATION_BOUNDS_NS:?}")
        .map_err(|error| error.to_string())?;
    for carrier in snapshot.carriers() {
        let id = carrier.id().index();
        let profile = carrier.handoff_profile();
        writeln!(
            output,
            "engine=vthread phase=handoff-routes carrier={id} local={} shared={}",
            profile.local_publications(),
            profile.remote_publications()
        )
        .map_err(|error| error.to_string())?;
        for stage in HandoffStage::ALL {
            let duration = profile.duration(stage);
            writeln!(output, "engine=vthread phase=handoff-duration carrier={id} stage={stage:?} count={} total_ns={} max_ns={} bins={:?}",
                duration.count(), duration.total_ns(), duration.maximum_ns(), duration.bins())
                .map_err(|error| error.to_string())?;
        }
        for direction in [ChannelDirection::Send, ChannelDirection::Receive] {
            let counts = profile.channel(direction);
            writeln!(output,
                "engine=vthread phase=handoff-channel carrier={id} direction={direction:?} calls={} transfers={} failed_calls={} park_calls={} park_returns={} parks={} retries={} resumed_transfers={} resumed_exits={} selected_notifications={} stored_notifications={} closed_notifications={} ineligible_notifications={}",
                counts.calls(), counts.transfers(), counts.failed_calls(), counts.park_calls(),
                counts.park_returns(), counts.parks(), counts.retries(), counts.resumed_transfers(),
                counts.resumed_exits(), counts.selected_notifications(), counts.stored_notifications(),
                counts.closed_notifications(), counts.ineligible_notifications())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn validate(snapshot: &RuntimeSnapshot, expected: Option<u64>) -> Result<(), String> {
    if snapshot.shutdown_phase() != ShutdownPhase::Complete || snapshot.active() != 0 {
        return Err("handoff profile requires completed shutdown and no active tasks".into());
    }
    let mut sent = 0;
    let mut received = 0;
    for carrier in snapshot.carriers() {
        let profile = carrier.handoff_profile();
        if carrier.status() != CarrierStatus::Stopped || !consistent(profile) {
            return Err(format!(
                "inconsistent handoff profile for {:?}",
                carrier.id()
            ));
        }
        let scheduler = carrier.scheduler_profile();
        let count = |stage| profile.duration(stage).count();
        if count(HandoffStage::IdleEpisode) != scheduler.idle_entries()
            || count(HandoffStage::PollWork) + count(HandoffStage::PollControl)
                != scheduler.poll_hits()
            || count(HandoffStage::PollExhausted) + scheduler.poll_hits()
                != scheduler.poll_episodes()
            || count(HandoffStage::WaitApi) != scheduler.wait_calls()
            || count(HandoffStage::IdlePublication) != scheduler.wait_calls()
        {
            return Err(format!(
                "inconsistent idle partition for {:?}",
                carrier.id()
            ));
        }
        sent += profile.channel(ChannelDirection::Send).transfers();
        received += profile.channel(ChannelDirection::Receive).transfers();
    }
    if let Some(expected) = expected
        && (sent != expected || received != expected)
    {
        return Err(format!(
            "expected {expected} channel transfers, observed {sent} sends and {received} receives"
        ));
    }
    Ok(())
}

fn consistent(profile: &HandoffProfile) -> bool {
    HandoffStage::ALL.into_iter().all(|stage| {
        let duration = profile.duration(stage);
        duration.count() == duration.bins().iter().sum::<u64>()
            && duration.maximum_ns() <= duration.total_ns()
    }) && [ChannelDirection::Send, ChannelDirection::Receive]
        .into_iter()
        .all(|direction| {
            let counts = profile.channel(direction);
            counts.calls() == counts.transfers() + counts.failed_calls()
                && counts.park_returns()
                    == counts.retries() + counts.resumed_transfers() + counts.resumed_exits()
                && counts.park_returns() <= counts.park_calls()
                && counts.parks() <= counts.park_calls()
                && counts.resumed_transfers() <= counts.transfers()
                && counts.ineligible_notifications()
                    <= counts.selected_notifications()
                        + counts.stored_notifications()
                        + counts.closed_notifications()
        })
}

#[cfg(test)]
#[path = "handoff_profile_test.rs"]
mod handoff_profile_test;
