use super::Semaphore;
use crate::{Error, Runtime, local_scope, yield_now};
use std::cell::RefCell;

#[test]
fn cancelled_task_cannot_consume_an_immediately_available_permit() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("cancelled", || {
                    let semaphore = Semaphore::with_wait_capacity(1, 1).unwrap();
                    crate::cancellation_token().unwrap().cancel();
                    assert!(matches!(semaphore.acquire(), Err(Error::Cancelled)));
                    assert_eq!(semaphore.available_permits(), 1);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn cancelling_a_selected_waiter_returns_its_permit_to_the_next_waiter() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let semaphore = Semaphore::with_wait_capacity(1, 2).unwrap();
                    let permit = semaphore.try_acquire().unwrap();
                    let token = RefCell::new(None);
                    local_scope(|local| {
                        let mut first = local.spawn("cancelled", || {
                            *token.borrow_mut() = Some(crate::cancellation_token().unwrap());
                            semaphore.acquire().map(drop)
                        })?;
                        let mut second =
                            local.spawn("successor", || semaphore.acquire().map(drop))?;
                        while semaphore.waiting() != 2 {
                            yield_now()?;
                        }
                        drop(permit);
                        token.borrow().as_ref().unwrap().cancel();
                        assert!(matches!(first.join()?, Err(Error::Cancelled)));
                        second.join()??;
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(semaphore.waiting(), 0);
                    assert_eq!(semaphore.available_permits(), 1);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn close_fails_waiters_and_held_permits_cannot_reopen_it() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let semaphore = Semaphore::with_wait_capacity(1, 1).unwrap();
                    let permit = semaphore.try_acquire().unwrap();
                    local_scope(|local| {
                        let mut waiter =
                            local.spawn("waiting", || semaphore.acquire().map(drop))?;
                        while semaphore.waiting() == 0 {
                            yield_now()?;
                        }
                        semaphore.close();
                        assert!(matches!(waiter.join()?, Err(Error::Closed)));
                        Ok(())
                    })
                    .unwrap();
                    drop(permit);
                    assert!(semaphore.is_closed());
                    assert_eq!(semaphore.available_permits(), 0);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn waiter_overflow_is_explicit_and_nonblocking_callers_do_not_barge() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let semaphore = Semaphore::with_wait_capacity(1, 1).unwrap();
                    let permit = semaphore.try_acquire().unwrap();
                    local_scope(|local| {
                        let mut waiter = local.spawn("first", || semaphore.acquire().map(drop))?;
                        while semaphore.waiting() == 0 {
                            yield_now()?;
                        }
                        assert!(matches!(
                            semaphore.acquire(),
                            Err(Error::Capacity {
                                resource: crate::error::CapacityResource::Waiters,
                                limit: 1
                            })
                        ));
                        drop(permit);
                        assert!(matches!(semaphore.try_acquire(), Err(Error::WouldBlock)));
                        waiter.join()??;
                        Ok(())
                    })
                    .unwrap();
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn multiple_permits_bound_concurrent_holders_across_carriers() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let runtime = Runtime::builder().carriers(4).build().unwrap();
    let semaphore = Arc::new(Semaphore::with_wait_capacity(3, 16).unwrap());
    let holders = Arc::new(AtomicUsize::new(0));
    runtime
        .run_scope(|scope| {
            let mut tasks = Vec::new();
            for _ in 0..16 {
                let semaphore = Arc::clone(&semaphore);
                let holders = Arc::clone(&holders);
                tasks.push(scope.spawn("holder", move || {
                    for _ in 0..32 {
                        let _permit = semaphore.acquire().unwrap();
                        assert!(holders.fetch_add(1, Ordering::SeqCst) < 3);
                        yield_now().unwrap();
                        holders.fetch_sub(1, Ordering::SeqCst);
                    }
                })?);
            }
            for mut task in tasks {
                task.join()?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(holders.load(Ordering::SeqCst), 0);
    assert_eq!(semaphore.available_permits(), 3);
    assert_eq!(semaphore.waiting(), 0);
}
