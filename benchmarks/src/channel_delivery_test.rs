use super::{Delivery, validate};
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
        sample_channel_latency: false,
    }
}

#[test]
fn exact_delivery_allows_arbitrary_interleaving_between_consumers() {
    let delivery = Delivery::new(vec![vec![4, 1, 3], vec![0, 5, 2]]);
    validate(&config(), Some(&delivery)).unwrap();
}

#[test]
fn missing_duplicate_out_of_range_and_incorrect_counts_are_rejected() {
    assert!(validate(&config(), None).is_err());
    for groups in [
        vec![vec![0, 1, 2]],
        vec![vec![0, 1], vec![2, 3, 4, 5]],
        vec![vec![0, 1, 2], vec![3, 4, 4]],
        vec![vec![0, 1, 2], vec![3, 4, 6]],
    ] {
        assert!(validate(&config(), Some(&Delivery::new(groups))).is_err());
    }
}

#[test]
fn other_scenarios_cannot_silently_ignore_delivery_evidence() {
    let mut config = config();
    config.scenario = Scenario::Spawn;
    validate(&config, None).unwrap();
    assert!(validate(&config, Some(&Delivery::new(Vec::new()))).is_err());
}

#[test]
fn measured_rounds_are_validated_as_well_as_warmup() {
    let mut calls = 0;
    let result = crate::report::measure(&config(), |_| {
        calls += 1;
        Ok(crate::report::Round {
            admission_ns: 0,
            operation_latency_groups_ns: Vec::new(),
            pair_owners: Vec::new(),
            task_migrations: Vec::new(),
            channel_delivery: (calls == 1)
                .then(|| Delivery::new(vec![vec![0, 1, 2], vec![3, 4, 5]])),
            #[cfg(feature = "lifecycle-profiling")]
            lifecycle: None,
        })
    });
    assert_eq!(calls, 2);
    assert!(result.unwrap_err().contains("evidence missing"));
}
