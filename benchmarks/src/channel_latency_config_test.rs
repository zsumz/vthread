use super::Config;

fn parse(arguments: &[&str]) -> Result<Config, String> {
    Config::parse_from(arguments.iter().map(|argument| (*argument).to_owned()))
}

#[test]
fn channel_sampling_is_explicit_and_does_not_change_the_transfer_denominator() {
    let arguments = ["vthread", "channel-mpmc", "10", "1", "4", "8", "3"];
    assert!(!parse(&arguments).unwrap().sample_channel_latency);
    for options in [
        vec![
            "--sample-channel-latency",
            "--pin-carriers",
            "--max-vthreads",
            "128",
        ],
        vec![
            "--max-vthreads",
            "128",
            "--pin-carriers",
            "--sample-channel-latency",
        ],
    ] {
        let mut sampled = arguments.to_vec();
        sampled.extend(options);
        let config = parse(&sampled).unwrap();
        assert!(config.sample_channel_latency);
        assert!(config.pin_carriers);
        assert_eq!(config.vthread_capacity(), 128);
        assert_eq!(config.operations(), 40);
        assert_eq!(
            config.operation(),
            "bounded-mpmc-channel-1-transfer-sampled"
        );
    }
}

#[test]
fn sampling_rejects_other_scenarios_and_duplicate_options() {
    for arguments in [
        vec!["vthread", "park", "10", "1", "4", "3"],
        vec!["vthread", "channel", "10", "1", "4", "3"],
        vec![
            "vthread",
            "channel-mpmc",
            "10",
            "1",
            "1",
            "4",
            "3",
            "--sample-channel-latency",
        ],
    ] {
        let mut arguments = arguments;
        arguments.push("--sample-channel-latency");
        assert!(parse(&arguments).is_err());
    }
}
