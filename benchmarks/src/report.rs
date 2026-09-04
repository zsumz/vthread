use crate::config::{Config, Scenario};
use std::{collections::BTreeMap, time::Instant};

pub(crate) struct Round {
    pub(crate) admission_ns: u128,
    pub(crate) operation_latency_groups_ns: Vec<Vec<u64>>,
    pub(crate) pair_owners: Vec<(usize, usize)>,
    #[cfg(feature = "lifecycle-profiling")]
    pub(crate) lifecycle: Option<vthread::diagnostics::LifecycleProfile>,
}

pub(crate) fn measure(
    config: &Config,
    mut round: impl FnMut(bool) -> Result<Round, String>,
) -> Result<(), String> {
    let warmup = round(true)?;
    let mut samples = Vec::with_capacity(config.samples);
    let mut admission_samples = Vec::with_capacity(config.samples);
    let mut drain_samples = Vec::with_capacity(config.samples);
    let mut operation_latency_groups = Vec::new();
    let pair_owners = warmup.pair_owners;
    #[cfg(feature = "allocation-probe")]
    let mut allocation_samples = Vec::with_capacity(config.samples);
    #[cfg(feature = "lifecycle-profiling")]
    let mut lifecycle_samples = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        #[cfg(feature = "allocation-probe")]
        crate::allocation_probe::begin();
        let started = Instant::now();
        let round = round(false)?;
        let total = started.elapsed().as_nanos();
        #[cfg(feature = "allocation-probe")]
        allocation_samples.push(crate::allocation_probe::finish());
        samples.push(total);
        admission_samples.push(round.admission_ns);
        drain_samples.push(total.saturating_sub(round.admission_ns));
        append_latency_groups(
            &mut operation_latency_groups,
            round.operation_latency_groups_ns,
        );
        assert!(
            round.pair_owners.is_empty(),
            "measured rounds must not collect placement diagnostics"
        );
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
    let (mut operation_latencies, task_medians, task_p99_9, task_maxima) =
        summarize_latency_groups(operation_latency_groups);
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
        print_task_fairness(config, &task_medians, &task_p99_9, &task_maxima);
        if matches!(config.scenario, Scenario::WakeTail { .. }) {
            print_pair_fairness(config, &task_p99_9, &task_maxima);
        }
    }
    if !pair_owners.is_empty() {
        print_placement(config, &pair_owners);
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

fn append_latency_groups(aggregate: &mut Vec<Vec<u64>>, mut round: Vec<Vec<u64>>) {
    if aggregate.is_empty() {
        *aggregate = round;
        return;
    }
    assert_eq!(aggregate.len(), round.len(), "latency group count changed");
    for (aggregate, samples) in aggregate.iter_mut().zip(&mut round) {
        aggregate.append(samples);
    }
}

fn summarize_latency_groups(groups: Vec<Vec<u64>>) -> (Vec<u64>, Vec<u64>, Vec<u64>, Vec<u64>) {
    let mut all = Vec::new();
    let mut medians = Vec::with_capacity(groups.len());
    let mut p99_9 = Vec::with_capacity(groups.len());
    let mut maxima = Vec::with_capacity(groups.len());
    for mut samples in groups {
        samples.sort_unstable();
        medians.push(latency_quantile(&samples, 50));
        p99_9.push(latency_quantile_ratio(&samples, 999, 1_000));
        maxima.push(*samples.last().expect("nonempty task latency samples"));
        all.append(&mut samples);
    }
    (all, medians, p99_9, maxima)
}

fn print_task_fairness(config: &Config, medians: &[u64], p99_9: &[u64], maxima: &[u64]) {
    let mut sorted_medians = medians.to_vec();
    let mut sorted_p99_9 = p99_9.to_vec();
    sorted_medians.sort_unstable();
    sorted_p99_9.sort_unstable();
    let (worst_task, worst_ns) = maxima
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, maximum)| *maximum)
        .expect("nonempty task latency groups");
    println!(
        "engine={} operation={} phase=fairness task_median_min_ns={} task_median_max_ns={} task_p99_9_min_ns={} task_p99_9_max_ns={} worst_task={} task_worst_ns={} task_streams={}",
        config.engine_name(),
        config.operation(),
        sorted_medians[0],
        sorted_medians[sorted_medians.len() - 1],
        sorted_p99_9[0],
        sorted_p99_9[sorted_p99_9.len() - 1],
        worst_task,
        worst_ns,
        medians.len(),
    );
}

fn print_pair_fairness(config: &Config, task_p99_9: &[u64], task_maxima: &[u64]) {
    let mut pair_p99_9 = pair_worst(task_p99_9);
    let mut pair_maxima = pair_worst(task_maxima);
    let (worst_pair, worst_ns) = pair_maxima
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, maximum)| *maximum)
        .expect("nonempty pair latency groups");
    pair_p99_9.sort_unstable();
    pair_maxima.sort_unstable();
    println!(
        "engine={} operation={} phase=pair-fairness pair_p99_9_min_ns={} pair_p99_9_median_ns={} pair_p99_9_max_ns={} pair_max_min_ns={} pair_max_median_ns={} pair_max_max_ns={} worst_pair={} pair_streams={}",
        config.engine_name(),
        config.operation(),
        pair_p99_9[0],
        latency_quantile(&pair_p99_9, 50),
        pair_p99_9[pair_p99_9.len() - 1],
        pair_maxima[0],
        latency_quantile(&pair_maxima, 50),
        worst_ns,
        worst_pair,
        pair_maxima.len(),
    );
}

fn pair_worst(task_values: &[u64]) -> Vec<u64> {
    assert!(
        task_values.len().is_multiple_of(2),
        "pair latency groups must be complete"
    );
    task_values
        .chunks_exact(2)
        .map(|pair| pair[0].max(pair[1]))
        .collect()
}

fn print_placement(config: &Config, pair_owners: &[(usize, usize)]) {
    let same = pair_owners
        .iter()
        .filter(|(left, right)| left == right)
        .count();
    let mut counts = BTreeMap::new();
    for owners in pair_owners {
        *counts.entry(*owners).or_insert(0_usize) += 1;
    }
    println!(
        "engine={} operation={} phase=placement same_carrier_pairs={} cross_carrier_pairs={} pair_observations={} owner_pair_counts={:?}",
        config.engine_name(),
        config.operation(),
        same,
        pair_owners.len() - same,
        pair_owners.len(),
        counts,
    );
}

fn per_operation(nanoseconds: u128, config: &Config) -> f64 {
    nanoseconds as f64 / config.operations() as f64
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
