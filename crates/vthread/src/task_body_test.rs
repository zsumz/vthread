use crate::{Error, Runtime, local_scope};

#[test]
fn unstarted_transferable_entry_drops_with_its_start_lease() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Count(Arc<AtomicUsize>);
    impl Drop for Count {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let capture = Count(Arc::clone(&drops));
    let (start, _outcome) = super::transferable(move || drop(capture));
    drop(start);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

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
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let result = local_scope(|local| {
                        drop(local.spawn("unjoined", || BadDrop)?);
                        Ok(())
                    });
                    assert!(matches!(
                        result.as_ref().map_err(crate::Error::primary),
                        Err(Error::TaskPanicked { .. })
                    ));
                    crate::yield_now().unwrap();
                })?
                .join()
        })
        .unwrap();
}
