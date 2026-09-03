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
fn wake_tail_requires_one_worker_and_pairs() {
    assert!(parse(&["vthread", "wake-tail", "10", "1", "2", "3"]).is_ok());
    assert!(parse(&["may", "wake-tail", "10", "2", "2", "3"]).is_err());
    assert!(parse(&["may", "wake-tail", "10", "1", "3", "3"]).is_err());
}

#[test]
fn operation_count_includes_each_task() {
    let config = parse(&["vthread", "mutex", "10", "1", "4", "3"]).unwrap();
    assert!(matches!(config.scenario, Scenario::Mutex { per_task: 10 }));
    assert_eq!(config.operations(), 40);
}

#[test]
fn mutex_requires_two_contending_tasks() {
    assert!(parse(&["vthread", "mutex", "10", "1", "2", "3"]).is_ok());
    assert!(parse(&["may", "mutex", "10", "1", "1", "3"]).is_err());
}

#[test]
fn tcp_accepts_unpaired_clients_and_counts_round_trips() {
    let config = parse(&["may", "tcp", "11", "2", "3", "5"]).unwrap();
    assert!(matches!(config.scenario, Scenario::Tcp { per_task: 11 }));
    assert_eq!(config.operations(), 33);
}
