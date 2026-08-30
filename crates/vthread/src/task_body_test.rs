use crate::{Error, Runtime, local_scope};

#[test]
fn unjoined_local_result_destructor_panics_are_owned_by_the_local_scope() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("local result destructor");
        }
    }
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
            scope
                .spawn("parent", || {
                    let result = local_scope(|local| {
                        drop(local.spawn("unjoined", || BadDrop)?);
                        Ok(())
                    });
                    assert!(matches!(result, Err(Error::TaskPanicked { .. })));
                    crate::yield_now().unwrap();
                })?
                .join()
        })
        .unwrap();
}
