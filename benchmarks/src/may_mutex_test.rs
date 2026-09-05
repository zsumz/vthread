use crate::config::{Config, Engine, Scenario};

#[test]
fn migration_probes_cover_odd_task_counts_only_during_warmup() {
    let config = Config {
        max_vthreads: None,
        engine: Engine::May,
        scenario: Scenario::Mutex {
            per_task: 10,
            contended: true,
        },
        workers: 1,
        tasks: 3,
        samples: 3,
    };
    let config = &config;
    let mut probes = Vec::new();
    may::coroutine::scope(|scope| {
        spawn_mutex_tasks!(scope, config, 10, true, true, probes);
    });
    assert_eq!(probes.len(), config.tasks);
    for probe in probes.drain(..) {
        let _ = probe.migrated();
    }
    may::coroutine::scope(|scope| {
        spawn_mutex_tasks!(scope, config, 10, true, false, probes);
    });
    assert!(probes.is_empty());
}
