use super::report;
use crate::config::{Config, Engine, Scenario};

fn config() -> Config {
    Config {
        engine: Engine::Vthread,
        scenario: Scenario::Spawn,
        workers: 2,
        tasks: 8,
        samples: 3,
        max_vthreads: Some(64),
        pin_carriers: false,
        sample_channel_latency: false,
    }
}

#[test]
fn final_report_checks_all_warmup_and_measured_tasks_without_round_observation() {
    let config = config();
    let runtime = crate::vthread_setup::build(&config).unwrap();
    for _ in 0..=config.samples {
        runtime
            .run_scope(|scope| {
                for _ in 0..config.tasks {
                    drop(scope.spawn("profile-test", || ())?);
                }
                Ok(())
            })
            .unwrap();
    }
    runtime.shutdown().unwrap();
    let mut output = Vec::new();
    report(&mut output, &runtime.snapshot(), &config).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("scope=whole-runtime headline=false rounds=4 expected_tasks=32"));
    assert_eq!(output.matches("phase=scheduler-admission").count(), 2);
    assert_eq!(output.matches("phase=scheduler-idle").count(), 2);
    assert!(output.contains("batch_bins=0,1,2-3,4-7,8-15,16-31,32-63,64+"));
}

#[test]
fn report_rejects_active_runtime_and_wrong_task_totals_before_writing() {
    let config = config();
    let runtime = crate::vthread_setup::build(&config).unwrap();
    let mut output = Vec::new();
    let active = report(&mut output, &runtime.snapshot(), &config);
    runtime.shutdown().unwrap();
    assert!(active.unwrap_err().contains("requires completed shutdown"));
    let error = report(&mut output, &runtime.snapshot(), &config).unwrap_err();
    assert!(error.contains("expected 32 tasks, observed 0 remote packets and 0 completions"));
    assert!(output.is_empty());
}

#[test]
fn report_rejects_overflowing_expected_counts() {
    let mut config = config();
    let runtime = crate::vthread_setup::build(&config).unwrap();
    runtime.shutdown().unwrap();
    config.samples = usize::MAX;
    let error = report(&mut Vec::new(), &runtime.snapshot(), &config).unwrap_err();
    assert_eq!(error, "scheduler profile task count overflow");
}

#[cfg(feature = "handoff-profiling")]
#[test]
fn mutex_expected_totals_include_warmup_and_reject_overflow() {
    let mut config = config();
    assert_eq!(super::mutex_acquisitions(&config).unwrap(), None);
    config.scenario = Scenario::Mutex {
        per_task: 1000,
        contended: true,
    };
    assert_eq!(super::mutex_acquisitions(&config).unwrap(), Some(32_000));
    config.scenario = Scenario::Mutex {
        per_task: usize::MAX,
        contended: false,
    };
    assert_eq!(
        super::mutex_acquisitions(&config).unwrap_err(),
        "handoff profile mutex acquisition count overflow"
    );
}
