use super::Notify;
use crate::{Error, Runtime, local_scope, yield_now};

#[test]
fn notifications_coalesce_only_when_no_waiter_is_available() {
    let notify = Notify::new(2).unwrap();
    notify.notify_one();
    notify.notify_one();
    notify.try_notified().unwrap();
    assert!(matches!(notify.try_notified(), Err(Error::WouldBlock)));
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let notify = Notify::new(2).unwrap();
                    local_scope(|local| {
                        let mut first = local.spawn("first", || notify.notified())?;
                        let mut second = local.spawn("second", || notify.notified())?;
                        while notify.waiting() != 2 {
                            yield_now()?;
                        }
                        notify.notify_one();
                        notify.notify_one();
                        first.join()??;
                        second.join()??;
                        assert!(matches!(notify.try_notified(), Err(Error::WouldBlock)));
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn broadcast_wakes_existing_waiters_only_and_close_is_terminal() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let notify = Notify::new(2).unwrap();
                    local_scope(|local| {
                        let mut first = local.spawn("first", || notify.notified())?;
                        let mut second = local.spawn("second", || notify.notified())?;
                        while notify.waiting() != 2 {
                            yield_now()?;
                        }
                        notify.notify_waiters();
                        first.join()??;
                        second.join()??;
                        assert!(matches!(notify.try_notified(), Err(Error::WouldBlock)));
                        let mut last = local.spawn("last", || notify.notified())?;
                        while notify.waiting() == 0 {
                            yield_now()?;
                        }
                        notify.close();
                        assert!(matches!(last.join()?, Err(Error::Closed)));
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn cancelled_selection_returns_one_notification_for_a_future_wait() {
    use std::cell::RefCell;
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let notify = Notify::new(1).unwrap();
                    let token = RefCell::new(None);
                    local_scope(|local| {
                        let mut child = local.spawn("cancelled", || {
                            *token.borrow_mut() = Some(crate::cancellation_token().unwrap());
                            notify.notified()
                        })?;
                        while notify.waiting() == 0 {
                            yield_now()?;
                        }
                        notify.notify_one();
                        token.borrow().as_ref().unwrap().cancel();
                        assert!(matches!(child.join()?, Err(Error::Cancelled)));
                        Ok(())
                    })
                    .unwrap();
                    notify.notified().unwrap();
                    assert!(matches!(notify.try_notified(), Err(Error::WouldBlock)));
                })?
                .join()
        })
        .unwrap();
}
