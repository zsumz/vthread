use super::{direction_summary, validate};
use crate::config::{Config, Engine, Scenario};

fn config() -> Config {
    Config {
        engine: Engine::Vthread,
        scenario: Scenario::ChannelMpmc {
            per_task: 3,
            capacity: 1,
        },
        workers: 1,
        tasks: 4,
        samples: 3,
        max_vthreads: None,
        pin_carriers: false,
        sample_channel_latency: true,
    }
}

#[test]
fn every_task_must_supply_exactly_one_sample_per_api_call() {
    let config = config();
    validate(&config, &vec![vec![1, 2, 3]; 4]).unwrap();
    for groups in [
        Vec::new(),
        vec![vec![1, 2, 3]; 3],
        vec![vec![1, 2, 3]; 5],
        vec![vec![1, 2]; 4],
        vec![vec![1, 2, 3, 4]; 4],
        vec![vec![1, 2, 3], vec![1, 2, 3], vec![1, 2], vec![1, 2, 3, 4]],
    ] {
        assert!(validate(&config, &groups).is_err());
    }
}

#[test]
fn untimed_and_unrelated_controls_cannot_silently_accept_channel_sampling() {
    let mut config = config();
    config.sample_channel_latency = false;
    validate(&config, &[]).unwrap();
    assert!(validate(&config, &vec![vec![1, 2, 3]; 4]).is_err());
    config.sample_channel_latency = true;
    config.engine = Engine::May;
    assert!(validate(&config, &vec![vec![1, 2, 3]; 4]).is_err());
    config.engine = Engine::Vthread;
    config.scenario = Scenario::Spawn;
    assert!(validate(&config, &[]).is_err());
}

#[test]
fn directional_report_keeps_slow_streams_and_nearest_rank_tails_visible() {
    let output = direction_summary(&config(), "send", &[vec![1, 2, 3], vec![10, 20, 100]]);
    assert!(output.contains("phase=channel-latency direction=send median_ns=3 p90_ns=100"));
    assert!(output.contains("p99_9_ns=100 p99_99_ns=100 max_ns=100 observations=6"));
    assert!(output.contains("task_median_min_ns=2 task_median_max_ns=20"));
    assert!(output.contains("task_p99_9_min_ns=3 task_p99_9_max_ns=100"));
    assert!(output.contains("worst_stream=1 task_worst_ns=100 task_streams=2"));
}

#[test]
fn missing_latency_evidence_is_rejected_in_warmup_and_measured_rounds() {
    for bad_round in [1, 2] {
        let mut calls = 0;
        let result = crate::report::measure(&config(), |_| {
            calls += 1;
            Ok(crate::report::Round {
                admission_ns: 0,
                operation_latency_groups_ns: if calls == bad_round {
                    Vec::new()
                } else {
                    vec![vec![1, 2, 3]; 4]
                },
                pair_owners: Vec::new(),
                task_migrations: Vec::new(),
                channel_delivery: Some(crate::channel_delivery::Delivery::new(vec![
                    vec![0, 1, 2],
                    vec![3, 4, 5],
                ])),
                #[cfg(feature = "lifecycle-profiling")]
                lifecycle: None,
            })
        });
        assert_eq!(calls, bad_round);
        assert!(result.unwrap_err().contains("every call in every task"));
    }
}
