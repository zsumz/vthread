use super::Completion;
use crate::{Error, wait::WaitCell};
use std::sync::Arc;

#[test]
fn subscriptions_are_bounded_and_unregister_on_drop() {
    let completion = Arc::new(Completion::new(1));
    let first = completion.subscribe(&WaitCell::new()).unwrap();
    assert!(matches!(
        completion.subscribe(&WaitCell::new()),
        Err(Error::AtCapacity { .. })
    ));
    drop(first);
    let _second = completion.subscribe(&WaitCell::new()).unwrap();
    completion.complete();
    assert!(completion.done());
    let _late = completion.subscribe(&WaitCell::new()).unwrap();
}
