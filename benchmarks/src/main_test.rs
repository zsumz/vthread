use super::run;

#[test]
fn command_keeps_the_explicit_runtime_prefix_and_runs_a_complete_sample() {
    run(["vthread", "spawn", "1", "2", "1"]
        .map(str::to_owned)
        .into_iter())
    .unwrap();
}

#[test]
fn malformed_commands_fail_before_starting_a_workload() {
    for arguments in [
        vec![],
        vec!["spawn", "1", "2", "1"],
        vec!["unknown", "spawn", "1", "2", "1"],
        vec!["vthread", "spawn", "1", "2", "2"],
    ] {
        assert!(run(arguments.into_iter().map(str::to_owned)).is_err());
    }
}
