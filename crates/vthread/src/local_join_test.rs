use crate::{Runtime, local_scope};

#[test]
fn dropping_a_local_join_handle_keeps_the_child_owned_by_its_scope() {
    let runtime = Runtime::new().unwrap();
    runtime
        .scope(|scope| {
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
