use super::{latency_quantile, latency_quantile_ratio, quantile};

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
