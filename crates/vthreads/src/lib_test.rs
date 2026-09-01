#[test]
fn reexports_the_vthread_runtime() {
    let answer = crate::run(|scope| {
        let mut task = scope.spawn("answer", || 42)?;
        task.join()
    })
    .unwrap();

    assert_eq!(answer, 42);
}
