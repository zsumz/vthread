use super::*;

#[test]
fn explicit_mechanisms_and_bounded_work_are_required() {
    for (name, mode) in [
        ("local", Mode::Local),
        ("remote-active", Mode::RemoteActive),
        ("remote-sleep-observed", Mode::RemoteSleepObserved),
    ] {
        let config = parse([name, "16"].map(String::from).into_iter()).unwrap();
        assert_eq!(config.mode, mode);
        assert_eq!(config.iterations, 16);
        assert!(config.pin);
    }
    for args in [
        vec![],
        vec!["local"],
        vec!["remote", "10"],
        vec!["local", "0"],
        vec!["local", "100001"],
        vec!["local", "2", "extra"],
    ] {
        assert!(parse(args.into_iter().map(String::from)).is_err());
    }
}
