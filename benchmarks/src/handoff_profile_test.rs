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
    let error = report(&mut output, &runtime.snapshot(), None).unwrap_err();
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
    let error = report(&mut output, &snapshot, Some(59)).unwrap_err();
    assert!(error.contains("observed 60 sends and 60 receives"));
    assert!(output.is_empty());
    report(&mut output, &snapshot, Some(60)).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("headline=false"));
    assert!(output.contains("regions_overlap=true"));
    assert_eq!(output.matches("phase=handoff-duration").count(), 22);
    assert_eq!(output.matches("phase=handoff-channel").count(), 4);
    assert_eq!(output.matches("phase=handoff-routes").count(), 2);
}
