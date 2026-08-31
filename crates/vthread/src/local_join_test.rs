use crate::{Runtime, local_scope};

#[test]
fn dropping_a_local_join_handle_keeps_the_child_owned_by_its_scope() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let mut value = 0;
                    local_scope(|local| {
                        std::mem::forget(local.spawn("forgotten", || value = 42)?);
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(value, 42);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn cancellation_of_a_local_wait_preserves_nonblocking_result_observation() {
    let runtime = Runtime::new().unwrap();
    runtime
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let result = local_scope(|local| {
                        let mut child = local.spawn("ready child", || 42)?;
                        while !child.is_finished() {
                            crate::yield_now()?;
                        }
                        crate::cancellation_token()?.cancel();
                        assert!(matches!(child.wait(), Err(crate::Error::Cancelled)));
                        assert_eq!(child.take_result()?, 42);
                        assert!(matches!(
                            child.take_result(),
                            Err(crate::Error::ResultAlreadyTaken)
                        ));
                        Ok(())
                    });
                    assert!(matches!(
                        result.as_ref().map_err(crate::Error::primary),
                        Err(crate::Error::Cancelled)
                    ));
                })?
                .join()
        })
        .unwrap();
}
