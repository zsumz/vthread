use crate::{RuntimeConfig, ScopeOptions, control::Shared};

#[test]
fn a_supervisor_does_not_take_the_lexical_scope_slot() {
    let shared = Shared::new(RuntimeConfig::default());
    let supervisor = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let lexical = shared.begin_scope().unwrap();
    shared.finish_scope(supervisor);
    assert!(shared.begin_scope().is_err());
    shared.finish_scope(lexical);
    assert!(shared.begin_scope().is_ok());
}
