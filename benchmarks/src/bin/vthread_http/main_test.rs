use super::{Config, DEFAULT_CONNECTIONS, DEFAULT_PORT};

fn parse(args: &[&str]) -> Result<Config, String> {
    Config::parse_from(args.iter().map(|value| (*value).to_owned()))
}

#[test]
fn explicit_configuration_is_parsed() {
    let config = parse(&["4", "8081", "512"]).unwrap();

    assert_eq!(config.workers, 4);
    assert_eq!(config.address.port(), 8081);
    assert_eq!(config.connections, 512);
}

#[test]
fn optional_values_have_bounded_defaults() {
    let config = parse(&["2"]).unwrap();

    assert_eq!(config.workers, 2);
    assert_eq!(config.address.port(), DEFAULT_PORT);
    assert_eq!(config.connections, DEFAULT_CONNECTIONS);
}

#[test]
fn zero_and_extra_arguments_are_rejected() {
    assert_eq!(
        parse(&["0"]).unwrap_err(),
        "workers must be a positive integer"
    );
    assert!(parse(&["1", "0"]).unwrap_err().contains("port"));
    assert!(parse(&["1", "8080", "2", "extra"]).is_err());
}
