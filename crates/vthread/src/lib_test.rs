use crate::{Error, run, yield_now};

#[test]
fn convenience_run_executes_a_scope() {
    let value = run(|scope| scope.spawn("answer", || 42)?.join()).expect("run succeeds");
    assert_eq!(value, 42);
}

#[test]
fn yielding_outside_a_virtual_thread_is_an_error() {
    assert!(matches!(yield_now(), Err(Error::OutsideVThread)));
}
