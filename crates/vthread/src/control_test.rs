use super::Shared;
use crate::{Error, RuntimeConfig};

#[test]
fn scope_admission_is_exclusive_and_shutdown_is_terminal() {
    let shared = Shared::new(RuntimeConfig::default());
    let scope = shared.begin_scope().expect("scope");
    assert!(matches!(shared.begin_scope(), Err(Error::RootScopeActive)));
    shared.finish_scope(scope);
    assert!(shared.begin_scope().is_ok());
    shared.request_stop();
    assert!(matches!(
        shared.submit(scope, "late".into(), || ()),
        Err(Error::RuntimeStopped)
    ));
    assert!(matches!(shared.begin_scope(), Err(Error::RuntimeStopped)));
}
