use crate::{
    channel_delivery::{Delivery, validate},
    config::{Config, Engine, Scenario},
};

#[test]
fn shared_channel_delivers_every_value_on_one_and_four_native_carriers() {
    for workers in [1, 4] {
        for capacity in [1, 64, 1_024] {
            let config = Config {
                engine: Engine::Vthread,
                scenario: Scenario::ChannelMpmc {
                    per_task: 64,
                    capacity,
                },
                workers,
                tasks: 8,
                samples: 3,
                max_vthreads: None,
                pin_carriers: false,
            };
            let runtime = crate::vthread_setup::build(&config).unwrap();
            let received = runtime
                .run_scope(|scope| {
                    let mut tasks = super::spawn_shared(scope, config.tasks, 64, capacity)?;
                    tasks
                        .iter_mut()
                        .map(|task| task.join()?)
                        .collect::<vthread::Result<Vec<_>>>()
                })
                .unwrap();
            runtime.shutdown().unwrap();
            validate(&config, Some(&Delivery::new(received))).unwrap();
            assert_eq!(runtime.snapshot().stats().completed(), 8);
        }
    }
}

#[test]
fn may_cannot_run_the_vthread_only_control_even_with_a_constructed_config() {
    let config = Config {
        engine: Engine::May,
        scenario: Scenario::ChannelMpmc {
            per_task: 1,
            capacity: 1,
        },
        workers: 1,
        tasks: 4,
        samples: 1,
        max_vthreads: None,
        pin_carriers: false,
    };
    assert!(
        crate::may_engine::run(&config)
            .unwrap_err()
            .contains("vthread-only")
    );
}
