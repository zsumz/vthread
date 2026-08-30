#[test]
fn invalid_workload_bounds_fail_before_starting_threads() {
    for args in [
        vec!["1", "0", "8", "500"],
        vec!["1", "257", "8", "500"],
        vec!["1", "8", "0", "500"],
        vec!["1", "8", "8", "0"],
        vec!["x"],
    ] {
        assert!(super::parse(args.into_iter().map(str::to_owned)).is_err());
    }
    assert!(super::parse(["4", "256", "32", "5000"].into_iter().map(str::to_owned)).is_ok());
}
