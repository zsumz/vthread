use super::report;
use vthread::Runtime;

#[test]
fn handoff_report_requires_final_shutdown_and_exact_transfers() {
    let runtime = Runtime::builder()
        .carriers(2)
        .max_vthreads(4)
        .carrier_queue_capacity(4)
        .stack_cache_capacity(4)
        .build()
        .unwrap();
    let mut output = Vec::new();
    let error = report(&mut output, &runtime.snapshot(), None, None).unwrap_err();
    assert!(error.contains("completed shutdown"));
    assert!(output.is_empty());
    for _ in 0..3 {
        runtime
            .run_scope(|scope| {
                let (sender, receiver) = vthread::channel::bounded(1)?;
                drop(scope.spawn("profile producer", move || {
                    for value in 0..20 {
                        sender.send(value).unwrap();
                    }
                })?);
                scope
                    .spawn("profile consumer", move || {
                        for value in 0..20 {
                            assert_eq!(receiver.recv().unwrap(), value);
                        }
                    })?
                    .join()?;
                Ok(())
            })
            .unwrap();
    }
    runtime.shutdown().unwrap();
    let snapshot = runtime.snapshot();
    let error = report(&mut output, &snapshot, Some(59), None).unwrap_err();
    assert!(error.contains("observed 60 sends and 60 receives"));
    assert!(output.is_empty());
    report(&mut output, &snapshot, Some(60), None).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("headline=false"));
    assert!(output.contains("regions_overlap=true"));
    assert_eq!(output.matches("phase=handoff-duration").count(), 22);
    assert_eq!(output.matches("phase=handoff-channel").count(), 4);
    assert_eq!(output.matches("phase=handoff-routes").count(), 2);
}

#[test]
fn mutex_report_requires_every_warmup_and_measured_acquisition() {
    use crate::config::{Config, Scenario};
    for workers in [1, 4] {
        let config = Config {
            scenario: Scenario::Mutex {
                per_task: 50,
                contended: true,
            },
            workers,
            tasks: 8,
            samples: 3,
            max_vthreads: Some(8),
            pin_carriers: false,
            sample_channel_latency: false,
        };
        let runtime = crate::vthread_setup::build(&config).unwrap();
        for _ in 0..=config.samples {
            crate::vthread_engine::profile_mutex_test_round(&runtime, &config).unwrap();
        }
        runtime.shutdown().unwrap();
        let snapshot = runtime.snapshot();
        let mut output = Vec::new();
        for wrong in [1599, 1601] {
            let error = report(&mut output, &snapshot, None, Some(wrong)).unwrap_err();
            assert!(error.contains("observed 1600 calls and 1600 acquisitions"));
            assert!(output.is_empty());
        }
        report(&mut output, &snapshot, None, Some(1600)).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("phase=handoff-mutex").count(), workers);
        assert!(output.contains("owner_identity=hub sleep_inferred=false"));
        let total = |field: fn(&vthread::diagnostics::MutexCounters) -> u64| {
            snapshot
                .carriers()
                .iter()
                .map(|carrier| field(carrier.handoff_profile().mutex()))
                .sum::<u64>()
        };
        use vthread::diagnostics::MutexCounters as Counts;
        assert_eq!(total(Counts::queued), total(Counts::completed));
        assert_eq!(total(Counts::completed), total(Counts::park_returns));
        assert_eq!(
            total(Counts::completed),
            total(Counts::stored) + total(Counts::same_owner) + total(Counts::other_owner)
        );
        if workers == 1 {
            assert_eq!(total(Counts::other_owner), 0);
            assert!(total(Counts::same_owner) > 0);
            assert_eq!(total(Counts::stored), 0);
            assert_eq!(total(Counts::parks), total(Counts::completed));
        }
    }
}
