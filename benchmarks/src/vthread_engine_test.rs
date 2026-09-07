use super::run_round;
use crate::config::{Config, Scenario};

#[test]
fn retained_scenarios_complete_and_keep_placement_outside_measured_rounds() {
    for (scenario, tasks) in [
        (Scenario::Yield { per_task: 3 }, 2),
        (Scenario::Spawn, 2),
        (Scenario::Park { per_task: 3 }, 2),
        (
            Scenario::Mutex {
                per_task: 3,
                contended: true,
            },
            2,
        ),
        (
            Scenario::Mutex {
                per_task: 3,
                contended: false,
            },
            1,
        ),
        (
            Scenario::Channel {
                per_task: 3,
                capacity: None,
            },
            2,
        ),
        (
            Scenario::Channel {
                per_task: 3,
                capacity: Some(2),
            },
            2,
        ),
        (
            Scenario::ChannelMpmc {
                per_task: 3,
                capacity: 2,
            },
            4,
        ),
        (Scenario::Tcp { per_task: 3 }, 1),
        (Scenario::WakeTail { per_task: 3 }, 2),
    ] {
        let config = Config {
            scenario,
            workers: 1,
            tasks,
            samples: 1,
            max_vthreads: None,
            pin_carriers: false,
            sample_channel_latency: false,
        };
        let runtime = crate::vthread_setup::build(&config).unwrap();
        for observe in [true, false] {
            let round = run_round(&runtime, &config, observe).unwrap();
            crate::channel_delivery::validate(&config, round.channel_delivery.as_ref()).unwrap();
            let paired = matches!(
                scenario,
                Scenario::Park { .. } | Scenario::Channel { .. } | Scenario::WakeTail { .. }
            );
            assert_eq!(
                round.pair_owners.len(),
                if observe && paired { tasks / 2 } else { 0 }
            );
            let timed = matches!(scenario, Scenario::Tcp { .. } | Scenario::WakeTail { .. });
            assert_eq!(
                round.operation_latency_groups_ns.len(),
                if timed { tasks } else { 0 }
            );
        }
        runtime.shutdown().unwrap();
        assert_eq!(runtime.snapshot().stats().completed(), (2 * tasks) as u64);
    }
}
