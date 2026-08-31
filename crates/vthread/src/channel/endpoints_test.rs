use crate::{Error, Runtime, channel::bounded_with_wait_capacity, local_scope, yield_now};

#[test]
fn last_sender_and_explicit_close_allow_buffer_drain() {
    for close in [true, false] {
        let (sender, receiver) = bounded_with_wait_capacity(2, 1).unwrap();
        sender.try_send(1).unwrap();
        sender.try_send(2).unwrap();
        if close {
            receiver.close();
        }
        drop(sender);
        assert!(receiver.is_closed());
        assert_eq!(receiver.try_recv().unwrap(), 1);
        assert_eq!(receiver.try_recv().unwrap(), 2);
        assert!(matches!(receiver.try_recv(), Err(Error::Closed)));
    }
}

#[test]
fn disconnect_wakes_blocked_senders_and_receivers() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded_with_wait_capacity(1, 1).unwrap();
                    sender.try_send(1).unwrap();
                    local_scope(|local| {
                        let mut child = local.spawn("send", || sender.send(2))?;
                        while sender.waiting() == 0 {
                            yield_now()?;
                        }
                        drop(receiver);
                        let error = child.join()?.unwrap_err();
                        assert!(matches!(error.error, Error::Closed));
                        assert_eq!(error.value, 2);
                        Ok(())
                    })
                    .unwrap();
                    let (sender, receiver) = bounded_with_wait_capacity::<u8>(1, 1).unwrap();
                    local_scope(|local| {
                        let mut child = local.spawn("receive", || receiver.recv())?;
                        while receiver.waiting() == 0 {
                            yield_now()?;
                        }
                        drop(sender);
                        assert!(matches!(child.join()?, Err(Error::Closed)));
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn buffered_destructors_can_reenter_the_channel() {
    use crate::channel::Sender;
    use std::sync::{Arc, Mutex};
    struct Reenter(Arc<Mutex<Option<Sender<Reenter>>>>);
    impl Drop for Reenter {
        fn drop(&mut self) {
            let guard = self.0.lock().unwrap();
            assert!(guard.as_ref().unwrap().is_closed());
        }
    }
    let (sender, receiver) = bounded_with_wait_capacity(1, 1).unwrap();
    let holder = Arc::new(Mutex::new(Some(sender.clone())));
    sender.try_send(Reenter(Arc::clone(&holder))).unwrap();
    drop(receiver);
    holder.lock().unwrap().take();
    assert!(sender.is_closed());
}
