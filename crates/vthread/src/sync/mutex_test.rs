use super::Mutex;
use crate::{Error, Runtime, local_scope, yield_now};
use std::{sync::Arc, thread};

#[test]
fn single_carrier_contention_is_fifo_and_guards_can_yield() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let mutex = Mutex::with_wait_capacity(Vec::new(), 4).unwrap();
                    let guard = mutex.lock().unwrap();
                    local_scope(|local| {
                        let mut children = Vec::new();
                        for i in 0..4 {
                            let mutex = &mutex;
                            children.push(local.spawn("contender", move || {
                                let owner = thread::current().id();
                                let mut guard = mutex.lock().unwrap();
                                yield_now().unwrap();
                                assert_eq!(owner, thread::current().id());
                                guard.push(i);
                            })?);
                            while mutex.waiting() != i + 1 {
                                yield_now()?;
                            }
                        }
                        drop(guard);
                        assert!(matches!(mutex.try_lock(), Err(Error::WouldBlock)));
                        for mut child in children {
                            child.join()?;
                        }
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(*mutex.lock().unwrap(), vec![0, 1, 2, 3]);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn multiple_carriers_exclude_each_other_and_preserve_updates() {
    let runtime = Runtime::builder().carriers(4).build().unwrap();
    let mutex = Arc::new(Mutex::with_wait_capacity(0, 16).unwrap());
    runtime
        .run_scope(|scope| {
            let mut children = Vec::new();
            for _ in 0..16 {
                let mutex = Arc::clone(&mutex);
                children.push(scope.spawn("increment", move || {
                    for _ in 0..64 {
                        let mut value = mutex.lock().unwrap();
                        let before = *value;
                        yield_now().unwrap();
                        *value = before + 1;
                    }
                })?);
            }
            for mut child in children {
                child.join()?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(*mutex.try_lock().unwrap(), 1024);
}

#[test]
fn panic_unlocks_and_returns_the_modified_value_without_poisoning() {
    let runtime = Runtime::new().unwrap();
    let mutex = Arc::new(Mutex::with_wait_capacity(0, 1).unwrap());
    runtime
        .run_scope(|scope| {
            let shared = Arc::clone(&mutex);
            let mut task = scope.spawn("panic", move || {
                *shared.lock().unwrap() = 42;
                let _guard = shared.lock().unwrap();
                panic!("owner failed");
            })?;
            assert!(matches!(task.join(), Err(Error::TaskPanicked { .. })));
            Ok(())
        })
        .unwrap();
    assert_eq!(*mutex.try_lock().unwrap(), 42);
}

#[test]
fn cancelling_a_selected_owner_hands_the_mutex_to_its_successor() {
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let mutex = Mutex::with_wait_capacity(0, 2).unwrap();
                    let guard = mutex.try_lock().unwrap();
                    local_scope(|local| {
                        let mut first = local.spawn("cancelled", || mutex.lock().map(drop))?;
                        let mut second = local.spawn("successor", || {
                            *mutex.lock()? = 42;
                            Ok::<_, Error>(())
                        })?;
                        while mutex.waiting() != 2 {
                            yield_now()?;
                        }
                        drop(guard);
                        first.cancel();
                        assert!(matches!(first.join()?, Err(Error::Cancelled)));
                        second.join()??;
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(*mutex.try_lock().unwrap(), 42);
                })?
                .join()
        })
        .unwrap();
}

#[test]
fn local_mutexes_can_protect_borrowed_non_send_values() {
    use std::{cell::Cell, rc::Rc};
    Runtime::new()
        .unwrap()
        .run_scope(|scope| {
            scope
                .spawn("parent", || {
                    let value = Rc::new(Cell::new(0));
                    let mutex = Mutex::with_wait_capacity(&value, 1).unwrap();
                    local_scope(|local| {
                        local
                            .spawn("borrower", || {
                                let guard = mutex.lock().unwrap();
                                yield_now().unwrap();
                                guard.set(42);
                            })?
                            .join()
                    })
                    .unwrap();
                    assert_eq!(value.get(), 42);
                })?
                .join()
        })
        .unwrap();
}
