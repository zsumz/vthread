use crate::{Error, Runtime, channel::bounded, local_scope, yield_now};
use std::cell::RefCell;

#[test]
fn blocked_senders_are_fifo_and_try_send_cannot_barge() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded(1, 3).unwrap();
                    sender.try_send(0).unwrap();
                    local_scope(|local| {
                        let mut first = local.spawn("first", || sender.send(1))?;
                        while sender.waiting() != 1 {
                            yield_now()?;
                        }
                        let mut second = local.spawn("second", || sender.send(2))?;
                        while sender.waiting() != 2 {
                            yield_now()?;
                        }
                        assert_eq!(receiver.recv()?, 0);
                        assert!(matches!(
                            sender.try_send(3).unwrap_err().error,
                            Error::WouldBlock
                        ));
                        assert_eq!(receiver.recv()?, 1);
                        assert_eq!(receiver.recv()?, 2);
                        first.join()?.unwrap();
                        second.join()?.unwrap();
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn selected_receiver_cancellation_preserves_the_buffer_and_fifo_successor() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded(1, 2).unwrap();
                    let token = RefCell::new(None);
                    local_scope(|local| {
                        let mut first = local.spawn("cancelled", || {
                            *token.borrow_mut() = Some(crate::cancellation_token().unwrap());
                            receiver.recv()
                        })?;
                        while receiver.waiting() != 1 {
                            yield_now()?;
                        }
                        let mut second = local.spawn("successor", || receiver.recv())?;
                        while receiver.waiting() != 2 {
                            yield_now()?;
                        }
                        sender.try_send(42).unwrap();
                        assert!(matches!(receiver.try_recv(), Err(Error::WouldBlock)));
                        token.borrow().as_ref().unwrap().cancel();
                        assert!(matches!(first.join()?, Err(Error::Cancelled)));
                        assert_eq!(second.join()??, 42);
                        Ok(())
                    })
                    .unwrap();
                    assert!(receiver.is_empty());
                    assert_eq!(receiver.waiting(), 0);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn cancelled_send_returns_its_input_even_after_capacity_wakes_it() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded(1, 1).unwrap();
                    sender.try_send(1).unwrap();
                    let token = RefCell::new(None);
                    local_scope(|local| {
                        let mut child = local.spawn("cancelled", || {
                            *token.borrow_mut() = Some(crate::cancellation_token().unwrap());
                            sender.send(42)
                        })?;
                        while sender.waiting() == 0 {
                            yield_now()?;
                        }
                        assert_eq!(receiver.recv()?, 1);
                        token.borrow().as_ref().unwrap().cancel();
                        let error = child.join()?.unwrap_err();
                        assert!(matches!(error.error, Error::Cancelled));
                        assert_eq!(error.value, 42);
                        Ok(())
                    })
                    .unwrap();
                    assert!(receiver.is_empty());
                    assert_eq!(sender.waiting(), 0);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn inherited_deadlines_remove_channel_waits_without_transferring_values() {
    use crate::local_scope_with_deadline;
    use std::time::{Duration, Instant};
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let (sender, receiver) = bounded(1, 1).unwrap();
                    sender.try_send(1).unwrap();
                    let error = local_scope_with_deadline(
                        Instant::now() + Duration::from_millis(20),
                        |local| {
                            let mut child = local.spawn("send-deadline", || sender.send(2))?;
                            let error = child.join()?.unwrap_err();
                            assert!(matches!(error.error, Error::DeadlineExceeded));
                            assert_eq!(error.value, 2);
                            Ok(())
                        },
                    )
                    .unwrap_err();
                    assert!(matches!(error.primary(), Error::DeadlineExceeded));
                    assert_eq!(sender.waiting(), 0);
                    assert_eq!(receiver.try_recv().unwrap(), 1);
                    let error = local_scope_with_deadline(
                        Instant::now() + Duration::from_millis(20),
                        |local| {
                            let mut child = local.spawn("recv-deadline", || receiver.recv())?;
                            assert!(matches!(child.join()?, Err(Error::DeadlineExceeded)));
                            Ok(())
                        },
                    )
                    .unwrap_err();
                    assert!(matches!(error.primary(), Error::DeadlineExceeded));
                    assert_eq!(receiver.waiting(), 0);
                    sender.try_send(42).unwrap();
                    assert_eq!(receiver.try_recv().unwrap(), 42);
                })?
                .join()
        })
        .unwrap();
}
