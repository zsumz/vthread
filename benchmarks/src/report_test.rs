use super::{
    append_latency_groups, latency_quantile, latency_quantile_ratio, pair_worst, quantile,
    summarize_latency_groups,
};

#[test]
fn quantiles_select_observed_values() {
    let samples: Vec<_> = (0..101).collect();
    assert_eq!(quantile(&samples, 50), 50);
    assert_eq!(quantile(&samples, 95), 95);
    assert_eq!(quantile(&samples, 99), 99);
}

#[test]
fn latency_quantiles_select_observed_values() {
    let samples: Vec<_> = (0..101).collect();
    assert_eq!(latency_quantile(&samples, 50), 50);
    assert_eq!(latency_quantile(&samples, 95), 95);
    assert_eq!(latency_quantile(&samples, 99), 99);
}

#[test]
fn high_quantiles_use_nearest_rank_for_short_samples() {
    let rounds: Vec<u128> = (1..=11).collect();
    let latencies: Vec<u64> = (1..=21).collect();
    assert_eq!(quantile(&rounds, 95), 11);
    assert_eq!(quantile(&rounds, 99), 11);
    assert_eq!(latency_quantile(&latencies, 95), 20);
    assert_eq!(latency_quantile(&latencies, 99), 21);
}

#[test]
fn fractional_tail_quantiles_use_nearest_rank() {
    let samples: Vec<u64> = (1..=10_000).collect();
    assert_eq!(latency_quantile_ratio(&samples, 999, 1_000), 9_990);
    assert_eq!(latency_quantile_ratio(&samples, 9_999, 10_000), 9_999);
}

#[test]
fn latency_groups_accumulate_by_task_and_preserve_fairness() {
    let mut groups = vec![vec![1, 3], vec![10, 30]];
    append_latency_groups(&mut groups, vec![vec![2, 4], vec![20, 40]]);
    let (mut all, medians, p99_9, maxima) = summarize_latency_groups(groups);
    all.sort_unstable();
    assert_eq!(all, vec![1, 2, 3, 4, 10, 20, 30, 40]);
    assert_eq!(medians, vec![2, 20]);
    assert_eq!(p99_9, vec![4, 40]);
    assert_eq!(maxima, vec![4, 40]);
}

#[test]
fn pair_worst_keeps_pair_order() {
    assert_eq!(pair_worst(&[3, 7, 11, 5]), vec![7, 11]);
}
