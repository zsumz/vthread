use super::TaskContext;
use crate::{SuspensionReason, options::TaskOptions};

#[test]
fn reuse_restores_policy_and_cold_defaults() {
    let root = crate::CancellationToken::root(4);
    let options = |cancellation| TaskOptions {
        cancellation,
        deadline: None,
    };
    let mut context = TaskContext::new(options(root.child_token()), 1);
    context.set_masked(2);
    context.replace_reason(SuspensionReason::YieldNow);
    context.close();

    context.recycle(root.child_token());
    let cancellation = root.child_token();
    context.reuse(options(cancellation.clone()), 3);

    assert!(context.check().is_ok());
    assert_eq!(context.masked(), 0);
    assert_eq!(context.reason(), SuspensionReason::Park);
    assert!(!context.closing());
    cancellation.cancel();
    assert!(matches!(context.check(), Err(crate::Error::Cancelled)));
}

#[test]
#[should_panic(expected = "reused task context crossed cancellation domains")]
fn reuse_rejects_a_different_runtime_domain() {
    let mut context = TaskContext::new(TaskOptions::root(crate::ScopeOptions::default(), 1), 1);
    context.reuse(TaskOptions::root(crate::ScopeOptions::default(), 1), 1);
}
