use crate::{Error, Runtime, park_pair};
use std::time::Duration;

#[test]
fn a_terminal_sibling_does_not_hide_an_indefinitely_parked_child() {
    let runtime = Runtime::builder()
        .carriers(2)
        .stall_timeout(Some(Duration::from_millis(10)))
        .build()
        .expect("runtime");
    let (parker, _unparker) = park_pair();
    let error = runtime
        .scope(|scope| {
            scope.spawn("parked", move || parker.park())?;
            scope.spawn("terminal", || 42)?.join()?;
            Ok(())
        })
        .expect_err("parked child must be reclaimed");
    assert!(matches!(error, Error::RuntimeStalled { active: 1 }));
    assert_eq!(runtime.snapshot().active, 0);
    runtime
        .scope(|scope| scope.spawn("reused", || ())?.join())
        .expect("reusable");
}
