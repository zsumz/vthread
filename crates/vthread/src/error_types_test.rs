use super::{ConfigurationField, LimitResource};
use crate::{Error, Runtime};

#[test]
fn invalid_configuration_is_matchable_without_parsing_text() {
    let error = Runtime::builder().io_capacity(0).build().unwrap_err();
    assert!(matches!(
        error,
        Error::InvalidConfiguration {
            field: ConfigurationField::IoCapacity,
            ..
        }
    ));
    assert_eq!(
        LimitResource::ResolvedAddresses.to_string(),
        "resolved addresses"
    );
}
