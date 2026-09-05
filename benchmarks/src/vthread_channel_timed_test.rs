use crate::{
    channel_delivery, channel_latency,
    config::{Config, Engine, Scenario},
};
use std::time::Instant;

#[test]
fn sampled_and_plain_controls_preserve_exact_delivery_on_native_carriers() {
    for workers in [1, 4] {
        for capacity in [1, 64, 1_024] {
            for sample_channel_latency in [false, true] {
                let config = Config {
                    engine: Engine::Vthread,
                    scenario: Scenario::ChannelMpmc {
                        per_task: 128,
                        capacity,
                    },
                    workers,
                    tasks: 8,
                    samples: 3,
                    max_vthreads: None,
                    pin_carriers: false,
                    sample_channel_latency,
                };
                let runtime = crate::vthread_setup::build(&config).unwrap();
                let shared = runtime
                    .run_scope(|scope| {
                        crate::vthread_channel::run_shared(
                            scope,
                            &config,
                            128,
                            capacity,
                            Instant::now(),
                        )
                    })
                    .unwrap();
                runtime.shutdown().unwrap();
                channel_delivery::validate(&config, Some(&shared.delivery)).unwrap();
                channel_latency::validate(&config, &shared.latency_groups_ns).unwrap();
                assert_eq!(
                    shared.latency_groups_ns.len(),
                    if sample_channel_latency { 8 } else { 0 }
                );
                assert_eq!(runtime.snapshot().stats().completed(), 8);
            }
        }
    }
}
