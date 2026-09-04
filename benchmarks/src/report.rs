use crate::config::{Config, Scenario};
use std::time::Instant;

pub(crate) struct Round {
    pub(crate) admission_ns: u128,
    pub(crate) operation_latencies_ns: Vec<u64>,
    #[cfg(feature = "lifecycle-profiling")]
    pub(crate) lifecycle: Option<vthread::diagnostics::LifecycleProfile>,
}

pub(crate) fn measure(
    config: &Config,
    mut round: impl FnMut() -> Result<Round, String>,
) -> Result<(), String> {
    round()?;
    let mut samples = Vec::with_capacity(config.samples);
    let mut admission_samples = Vec::with_capacity(config.samples);
    let mut drain_samples = Vec::with_capacity(config.samples);
    let mut operation_latencies = Vec::new();
    #[cfg(feature = "allocation-probe")]
    let mut allocation_samples = Vec::with_capacity(config.samples);
    #[cfg(feature = "lifecycle-profiling")]
    let mut lifecycle_samples = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        #[cfg(feature = "allocation-probe")]
        crate::allocation_probe::begin();
        let started = Instant::now();
        let round = round()?;
        let total = started.elapsed().as_nanos();
        #[cfg(feature = "allocation-probe")]
        allocation_samples.push(crate::allocation_probe::finish());
        samples.push(total);
        admission_samples.push(round.admission_ns);
        drain_samples.push(total.saturating_sub(round.admission_ns));
        operation_latencies.extend(round.operation_latencies_ns);
        #[cfg(feature = "lifecycle-profiling")]
        if let Some(profile) = round.lifecycle {
            lifecycle_samples.push(crate::lifecycle_profile::Sample::new(
                profile,
                total,
                round.admission_ns,
                config.tasks,
            )?);
        }
    }
    samples.sort_unstable();
    admission_samples.sort_unstable();
    drain_samples.sort_unstable();
    let median = quantile(&samples, 50);
    let p95 = quantile(&samples, 95);
    let p99 = quantile(&samples, 99);
    let maximum = *samples.last().expect("positive sample count");
    println!(
        "engine={} operation={} workers={} tasks={} median_ns={} ns_per_operation={:.2} p95_ns={} p95_ns_per_operation={:.2} p99_ns={} p99_ns_per_operation={:.2} max_ns={} samples={:?}",
        config.engine_name(),
        config.operation(),
        config.workers,
        config.tasks,
        median,
        per_operation(median, config),
        p95,
        per_operation(p95, config),
        p99,
        per_operation(p99, config),
        maximum,
        samples,
    );
    if matches!(config.scenario, Scenario::Spawn) {
        println!(
            "engine={} phase=spawn workers={} tasks={} admission_median_ns={} drain_median_ns={} admission_samples={:?} drain_samples={:?}",
            config.engine_name(),
            config.workers,
            config.tasks,
            admission_samples[admission_samples.len() / 2],
            drain_samples[drain_samples.len() / 2],
            admission_samples,
            drain_samples,
        );
    }
    if !operation_latencies.is_empty() {
        operation_latencies.sort_unstable();
        let median = latency_quantile(&operation_latencies, 50);
        let p95 = latency_quantile(&operation_latencies, 95);
        let p99 = latency_quantile(&operation_latencies, 99);
        let p99_9 = latency_quantile_ratio(&operation_latencies, 999, 1_000);
        let p99_99 = latency_quantile_ratio(&operation_latencies, 9_999, 10_000);
        let maximum = *operation_latencies
            .last()
            .expect("nonempty latency samples");
        println!(
            "engine={} operation={} phase=latency median_ns={} p95_ns={} p99_ns={} p99_9_ns={} p99_99_ns={} max_ns={} observations={}",
            config.engine_name(),
            config.operation(),
            median,
            p95,
            p99,
            p99_9,
            p99_99,
            maximum,
            operation_latencies.len(),
        );
    }
    #[cfg(feature = "allocation-probe")]
    crate::allocation_probe::print_medians(config, &mut allocation_samples);
    #[cfg(feature = "lifecycle-profiling")]
    crate::lifecycle_profile::print_medians(config, &lifecycle_samples);
    Ok(())
}

fn quantile(samples: &[u128], percentile: usize) -> u128 {
    let index = (samples.len() * percentile).div_ceil(100) - 1;
    samples[index]
}

fn latency_quantile(samples: &[u64], percentile: usize) -> u64 {
    latency_quantile_ratio(samples, percentile, 100)
}

fn latency_quantile_ratio(samples: &[u64], numerator: usize, denominator: usize) -> u64 {
    assert!(!samples.is_empty(), "latency samples must not be empty");
    assert!(numerator > 0, "quantile numerator must be positive");
    assert!(numerator <= denominator, "quantile must not exceed one");
    let rank =
        ((samples.len() as u128) * (numerator as u128)).div_ceil(denominator as u128) as usize;
    let index = rank - 1;
    samples[index]
}

fn per_operation(nanoseconds: u128, config: &Config) -> f64 {
    nanoseconds as f64 / config.operations() as f64
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
