use super::{Config, Scenario};

fn parse(arguments: &[&str]) -> Result<Config, String> {
    Config::parse_from(arguments.iter().map(|argument| (*argument).to_owned()))
}

#[test]
fn paired_scenarios_require_even_task_counts() {
    assert!(parse(&["vthread", "park", "10", "1", "2", "3"]).is_ok());
    assert!(parse(&["may", "channel", "10", "1", "3", "3"]).is_err());
}

#[test]
fn bounded_spsc_channels_require_and_report_their_capacity() {
    let config = parse(&["may", "channel-bounded-spsc", "10", "64", "4", "8", "3"]).unwrap();
    assert!(matches!(
        config.scenario,
        Scenario::Channel {
            per_task: 10,
            capacity: Some(64)
        }
    ));
    assert_eq!(config.operation(), "bounded-spsc-channel-64-handoff");
    assert!(parse(&["may", "channel-bounded-spsc", "10", "0", "4", "8", "3"]).is_err());
    let excessive = isize::MAX.to_string();
    assert!(
        Config::parse_from(
            [
                "may".to_owned(),
                "channel-bounded-spsc".to_owned(),
                "10".to_owned(),
                excessive,
                "4".to_owned(),
                "8".to_owned(),
                "3".to_owned(),
            ]
            .into_iter(),
        )
        .is_err()
    );
}

#[test]
fn wake_tail_accepts_multiple_workers_but_still_requires_pairs() {
    assert!(parse(&["vthread", "wake-tail", "10", "1", "2", "3"]).is_ok());
    assert!(parse(&["may", "wake-tail", "10", "2", "2", "3"]).is_ok());
    assert!(parse(&["may", "wake-tail", "10", "1", "3", "3"]).is_err());
}

#[test]
fn operation_count_includes_each_task() {
    let config = parse(&["vthread", "mutex", "10", "1", "4", "3"]).unwrap();
    assert!(matches!(
        config.scenario,
        Scenario::Mutex {
            per_task: 10,
            contended: true
        }
    ));
    assert_eq!(config.operations(), 40);
}

#[test]
fn mutex_requires_two_contending_tasks() {
    assert!(parse(&["vthread", "mutex", "10", "1", "2", "3"]).is_ok());
    assert!(parse(&["may", "mutex", "10", "1", "1", "3"]).is_err());
}

#[test]
fn uncontended_mutex_requires_exactly_one_task() {
    let config = parse(&["vthread", "mutex-uncontended", "10", "1", "1", "3"]).unwrap();
    assert!(matches!(
        config.scenario,
        Scenario::Mutex {
            per_task: 10,
            contended: false
        }
    ));
    assert_eq!(config.operation(), "mutex-uncontended");
    assert!(parse(&["may", "mutex-uncontended", "10", "1", "2", "3"]).is_err());
}

#[test]
fn tcp_accepts_unpaired_clients_and_counts_round_trips() {
    let config = parse(&["may", "tcp", "11", "2", "3", "5"]).unwrap();
    assert!(matches!(config.scenario, Scenario::Tcp { per_task: 11 }));
    assert_eq!(config.operations(), 33);
}

#[test]
fn runtime_capacity_can_exceed_the_live_task_count_without_changing_work() {
    let config = parse(&[
        "vthread",
        "park",
        "100",
        "4",
        "64",
        "9",
        "--max-vthreads",
        "65536",
    ])
    .unwrap();
    assert_eq!(config.tasks, 64);
    assert_eq!(config.operations(), 6400);
    assert_eq!(config.max_vthreads, Some(65536));
    assert_eq!(
        parse(&["vthread", "park", "100", "4", "64", "9"])
            .unwrap()
            .max_vthreads,
        None
    );
}

#[test]
fn runtime_capacity_override_is_validated_and_never_silently_applied_to_may() {
    for capacity in ["0", "1", "63", "invalid"] {
        assert!(
            parse(&[
                "vthread",
                "park",
                "100",
                "4",
                "64",
                "9",
                "--max-vthreads",
                capacity,
            ])
            .is_err()
        );
    }
    assert!(
        parse(&[
            "vthread",
            "park",
            "100",
            "4",
            "2",
            "9",
            "--max-vthreads",
            "2",
        ])
        .is_err()
    );
    assert!(
        parse(&[
            "may",
            "park",
            "100",
            "4",
            "64",
            "9",
            "--max-vthreads",
            "65536",
        ])
        .is_err()
    );
    assert!(
        parse(&[
            "vthread",
            "park",
            "100",
            "4",
            "64",
            "9",
            "--max-vthreads",
            "65536",
            "extra",
        ])
        .is_err()
    );
}
