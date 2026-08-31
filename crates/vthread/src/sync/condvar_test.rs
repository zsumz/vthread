use super::Condvar;
use crate::{Error, Runtime, local_scope, sync::Mutex, yield_now};
use std::{cell::RefCell, sync::Arc};

#[test]
fn predicate_wait_releases_and_reacquires_on_one_carrier() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let mutex = Mutex::with_wait_capacity(false, 2).unwrap();
                    let changed = Condvar::with_wait_capacity(2).unwrap();
                    changed.notify_one(); // No stored notification.
                    local_scope(|local| {
                        let mut waiter = local.spawn("predicate", || {
                            let mut value = mutex.lock().unwrap();
                            while !*value {
                                value = changed.wait(value).unwrap();
                            }
                        })?;
                        while changed.waiting() == 0 {
                            yield_now()?;
                        }
                        let mut guard = mutex.lock()?;
                        *guard = true;
                        changed.notify_one();
                        yield_now()?; // The waiter must now park for the still-held mutex.
                        drop(guard);
                        waiter.join()?;
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn cancellation_removes_the_condition_wait_and_leaves_mutex_unlocked() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let mutex = Mutex::with_wait_capacity(0, 1).unwrap();
                    let changed = Condvar::with_wait_capacity(1).unwrap();
                    let token = RefCell::new(None);
                    local_scope(|local| {
                        let mut waiter = local.spawn("cancelled", || {
                            *token.borrow_mut() = Some(crate::cancellation_token().unwrap());
                            changed.wait(mutex.lock().unwrap()).map(drop)
                        })?;
                        while changed.waiting() == 0 {
                            yield_now()?;
                        }
                        token.borrow().as_ref().unwrap().cancel();
                        assert!(matches!(waiter.join()?, Err(Error::Cancelled)));
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(changed.waiting(), 0);
                    assert_eq!(*mutex.try_lock().unwrap(), 0);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn registration_and_unlock_do_not_lose_remote_notifications() {
    let runtime = Runtime::builder().carriers(2).build().unwrap();
    for _ in 0..64 {
        let pair = Arc::new((
            Mutex::with_wait_capacity(false, 2).unwrap(),
            Condvar::with_wait_capacity(2).unwrap(),
        ));
        runtime
            .run_scope(|scope| {
                let shared = Arc::clone(&pair);
                let mut waiter = scope.spawn("waiter", move || {
                    let mut ready = shared.0.lock().unwrap();
                    while !*ready {
                        ready = shared.1.wait(ready).unwrap();
                    }
                })?;
                let shared = Arc::clone(&pair);
                scope
                    .spawn("notifier", move || {
                        *shared.0.lock().unwrap() = true;
                        shared.1.notify_all();
                    })?
                    .join()?;
                waiter.join()
            })
            .unwrap();
    }
}
