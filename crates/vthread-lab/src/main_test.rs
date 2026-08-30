#[test]
fn malformed_and_unbounded_runs_are_rejected() {
    for args in [
        "",
        "soak 0 1 4",
        "soak 86401 1 4",
        "soak 1 0 4",
        "soak 1 1 999999",
        "soak 1 1 4 extra",
    ] {
        assert!(super::parse_args(args.split_whitespace().map(str::to_owned)).is_err());
    }
    assert!(super::parse_args("soak 1 4 64".split_whitespace().map(str::to_owned)).is_ok());
}
