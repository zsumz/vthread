use crate::{Error, Runtime, local_scope};

#[test]
fn unstarted_transferable_entry_drops_with_shared_or_unique_start_ownership() {
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
    for unique in [false, true] {
        let capture = Count(Arc::clone(&drops));
        let (start, outcome) = super::transferable(move || drop(capture));
        if unique {
            drop(outcome);
            drop(start);
        } else {
            drop(start);
            drop(outcome);
        }
    }
    assert_eq!(drops.load(Ordering::SeqCst), 2);
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
