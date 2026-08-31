use crate::{Error, Runtime, channel::bounded, local_scope, yield_now};

#[test]
fn waiter_bounds_include_selected_sends_and_receives() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded(1, 1).unwrap();
                    local_scope(|local| {
                        let mut first = local.spawn("receive", || receiver.recv())?;
                        while receiver.waiting() == 0 {
                            yield_now()?;
                        }
                        sender.try_send(1).unwrap();
                        assert!(matches!(
                            receiver.recv(),
                            Err(Error::Capacity {
                                resource: crate::error::CapacityResource::Waiters,
                                limit: 1
                            })
                        ));
                        assert_eq!(first.join()??, 1);
                        sender.try_send(2).unwrap();
                        let mut second = local.spawn("send", || sender.send(3))?;
                        while sender.waiting() == 0 {
                            yield_now()?;
                        }
                        assert_eq!(receiver.recv()?, 2);
                        let error = sender.send(4).unwrap_err();
                        assert!(matches!(
                            error.error,
                            Error::Capacity {
                                resource: crate::error::CapacityResource::Waiters,
                                limit: 1
                            }
                        ));
                        assert_eq!(error.value, 4);
                        second.join()?.unwrap();
                        assert_eq!(receiver.recv()?, 3);
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn shutdown_removes_both_directions_and_reclaims_unsent_input() {
    use crate::support_test::until;
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
    let runtime = Runtime::new().unwrap();
    let (sender, receiver) = bounded(1, 1).unwrap();
    let (_other_sender, other_receiver) = bounded::<u8>(1, 1).unwrap();
    sender.try_send(Count(Arc::clone(&drops))).unwrap();
    runtime
        .run_scope(|scope| {
            let sending = sender.clone();
            let tracked = Arc::clone(&drops);
            let mut send_task = scope.spawn("send", move || sending.send(Count(tracked)))?;
            let receiving = other_receiver.clone();
            let mut recv_task = scope.spawn("recv", move || receiving.recv())?;
            until(|| sender.waiting() == 1 && other_receiver.waiting() == 1);
            runtime.shutdown()?;
            assert!(matches!(send_task.join(), Err(Error::TaskAborted { .. })));
            assert!(matches!(recv_task.join(), Err(Error::TaskAborted { .. })));
            Ok(())
        })
        .unwrap();
    assert_eq!(sender.waiting(), 0);
    assert_eq!(other_receiver.waiting(), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(receiver);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
