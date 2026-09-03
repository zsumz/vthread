use super::TaskContext;
use crate::{ScopeOptions, SuspensionReason, options::TaskOptions};

#[test]
fn reuse_restores_policy_and_cold_defaults() {
    let mut context = TaskContext::new(TaskOptions::root(ScopeOptions::default(), 1), 1);
    context.set_masked(2);
    context.replace_reason(SuspensionReason::YieldNow);
    context.close();

    context.recycle(crate::CancellationToken::root(1));
    context.reuse(TaskOptions::root(ScopeOptions::default(), 1), 3);

    assert!(context.check().is_ok());
    assert_eq!(context.masked(), 0);
    assert_eq!(context.reason(), SuspensionReason::Park);
    assert!(!context.closing());
}
