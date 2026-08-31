use super::SupervisorTimeout;
use crate::{RuntimeConfig, ScopeOptions, control::Shared, diagnostics::ScopeId};

#[test]
fn selection_keeps_other_owners_only_in_the_full_snapshot() {
    let shared = Shared::new(RuntimeConfig::default());
    let first = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    let second = shared.begin_owned(ScopeOptions::default(), true).unwrap();
    shared.submit(first, "mine".into(), || ()).unwrap();
    shared.submit(second, "other".into(), || ()).unwrap();
    let timeout = SupervisorTimeout::new(ScopeId::new(first), shared.snapshot());
    assert_eq!(timeout.supervisor_id(), ScopeId::new(first));
    assert_eq!(timeout.runtime_snapshot().tasks().len(), 2);
    assert_eq!(
        timeout.tasks().map(|task| task.name()).collect::<Vec<_>>(),
        ["mine"]
    );
}
