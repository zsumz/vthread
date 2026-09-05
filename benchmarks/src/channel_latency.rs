//! Contract checks and direction-specific summaries for sampled channel calls.

use crate::{
    config::{Config, Engine, Scenario},
    report::{latency_quantile_ratio, summarize_latency_groups},
};

pub(crate) fn validate(config: &Config, groups: &[Vec<u64>]) -> Result<(), String> {
    if !config.sample_channel_latency {
        return if matches!(config.scenario, Scenario::ChannelMpmc { .. }) && !groups.is_empty() {
            Err("unexpected channel latency samples in the untimed control".into())
        } else {
            Ok(())
        };
    }
    let Scenario::ChannelMpmc { per_task, .. } = config.scenario else {
        return Err("channel latency sampling requires the shared MPMC control".into());
    };
    if !matches!(config.engine, Engine::Vthread) {
        return Err("channel latency sampling is a vthread-only control".into());
    }
    if groups.len() != config.tasks || groups.iter().any(|group| group.len() != per_task) {
        return Err("channel latency evidence must cover every call in every task".into());
    }
    Ok(())
}

pub(crate) fn print_contract() {
    println!(
        "engine=vthread phase=channel-latency-contract measurement=endpoint-api-call clock=Instant clock_overhead=included samples_per_transfer=2 recording=inside-elapsed distribution=closed-loop message_delivery_latency=false open_loop=false stream_order=consumers-then-producers streams=logical-admission-slots fairness=latency-spread-not-progress-bound"
    );
}

pub(crate) fn print_directions(config: &Config, groups: &[Vec<u64>]) {
    if !config.sample_channel_latency {
        return;
    }
    let (receivers, senders) = groups.split_at(config.tasks / 2);
    for (direction, streams) in [("receive", receivers), ("send", senders)] {
        print!("{}", direction_summary(config, direction, streams));
    }
}

fn direction_summary(config: &Config, direction: &str, groups: &[Vec<u64>]) -> String {
    let (mut all, medians, tails, maxima) = summarize_latency_groups(groups.to_vec());
    all.sort_unstable();
    let (worst_stream, worst_ns) = maxima
        .iter()
        .enumerate()
        .max_by_key(|(_, value)| *value)
        .expect("validated nonempty channel latency streams");
    let quantile = |numerator, denominator| latency_quantile_ratio(&all, numerator, denominator);
    format!(
        "engine={} operation={} phase=channel-latency direction={direction} median_ns={} p90_ns={} p95_ns={} p99_ns={} p99_9_ns={} p99_99_ns={} max_ns={} observations={}\nengine={} operation={} phase=channel-fairness direction={direction} task_median_min_ns={} task_median_max_ns={} task_p99_9_min_ns={} task_p99_9_max_ns={} worst_stream={worst_stream} task_worst_ns={worst_ns} task_streams={}\n",
        config.engine_name(),
        config.operation(),
        quantile(50, 100),
        quantile(90, 100),
        quantile(95, 100),
        quantile(99, 100),
        quantile(999, 1_000),
        quantile(9_999, 10_000),
        all.last()
            .expect("validated nonempty channel latency samples"),
        all.len(),
        config.engine_name(),
        config.operation(),
        medians.iter().min().unwrap(),
        medians.iter().max().unwrap(),
        tails.iter().min().unwrap(),
        tails.iter().max().unwrap(),
        groups.len(),
    )
}

#[cfg(test)]
#[path = "channel_latency_test.rs"]
mod channel_latency_test;
