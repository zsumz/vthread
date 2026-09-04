use super::bounded_with_wait_capacity;
use crate::{Error, Runtime};
use std::sync::{Arc, Mutex};

#[test]
fn simple_channel_constructor_exposes_a_bounded_default() {
    assert!(super::bounded::<u8>(0).is_err());
    let (sender, receiver) = super::bounded(1).unwrap();
    assert_eq!(sender.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    assert_eq!(receiver.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    sender.try_send(42).unwrap();
    assert_eq!(receiver.try_recv().unwrap(), 42);
    assert_eq!(sender.capacity(), 1);
    let (sender, receiver) = bounded_with_wait_capacity::<u8>(1, 7).unwrap();
    assert_eq!(sender.wait_capacity(), 7);
    assert_eq!(receiver.wait_capacity(), 7);
}

#[test]
fn mpmc_delivers_each_value_once_on_one_and_four_carriers() {
    for carriers in [1, 4] {
        let runtime = Runtime::builder().carriers(carriers).build().unwrap();
        let (sender, receiver) = bounded_with_wait_capacity(3, 8).unwrap();
        let output = Arc::new(Mutex::new(Vec::new()));
        runtime
            .run_scope(|scope| {
                let mut tasks = Vec::new();
                for _ in 0..4 {
                    let receiver = receiver.clone();
                    let output = Arc::clone(&output);
                    tasks.push(scope.spawn("consumer", move || {
                        loop {
                            match receiver.recv() {
                                Ok(value) => output.lock().unwrap().push(value),
                                Err(Error::Closed) => break,
                                Err(error) => panic!("unexpected receive failure: {error}"),
                            }
                            crate::yield_now().unwrap();
                        }
                    })?);
                }
                for producer in 0..4 {
                    let sender = sender.clone();
                    tasks.push(scope.spawn("producer", move || {
                        for value in 0..128 {
                            sender.send(producer * 128 + value).unwrap();
                            crate::yield_now().unwrap();
                        }
                    })?);
                }
                drop(sender);
                drop(receiver);
                for mut task in tasks {
                    task.join()?;
                }
                Ok(())
            })
            .unwrap();
        let mut actual = output.lock().unwrap();
        actual.sort_unstable();
        assert_eq!(*actual, (0..512).collect::<Vec<_>>());
    }
}

#[test]
fn native_try_operations_remain_exact_under_contention() {
    const PRODUCERS: usize = 4;
    const VALUES: usize = 1_024;
    let (sender, receiver) = bounded_with_wait_capacity(16, 1).unwrap();
    let output = Arc::new(Mutex::new(Vec::with_capacity(PRODUCERS * VALUES)));
    std::thread::scope(|threads| {
        for _ in 0..PRODUCERS {
            let receiver = receiver.clone();
            let output = Arc::clone(&output);
            threads.spawn(move || {
                loop {
                    match receiver.try_recv() {
                        Ok(value) => output.lock().unwrap().push(value),
                        Err(Error::WouldBlock) => std::thread::yield_now(),
                        Err(Error::Closed) => break,
                        Err(error) => panic!("unexpected receive failure: {error}"),
                    }
                }
            });
        }
        for producer in 0..PRODUCERS {
            let sender = sender.clone();
            threads.spawn(move || {
                for offset in 0..VALUES {
                    let mut value = producer * VALUES + offset;
                    loop {
                        match sender.try_send(value) {
                            Ok(()) => break,
                            Err(error) if matches!(error.error(), Error::WouldBlock) => {
                                value = error.into_inner();
                                std::thread::yield_now();
                            }
                            Err(error) => panic!("unexpected send failure: {error}"),
                        }
                    }
                }
            });
        }
        drop(sender);
        drop(receiver);
    });
    let mut actual = Arc::into_inner(output).unwrap().into_inner().unwrap();
    actual.sort_unstable();
    assert_eq!(actual, (0..PRODUCERS * VALUES).collect::<Vec<_>>());
}

#[test]
fn constructors_and_nonblocking_operations_enforce_bounds() {
    assert!(bounded_with_wait_capacity::<u8>(0, 1).is_err());
    assert!(bounded_with_wait_capacity::<u8>(1, 0).is_err());
    let (sender, receiver) = bounded_with_wait_capacity(1, 1).unwrap();
    assert!(matches!(receiver.recv(), Err(Error::OutsideVThread)));
    let error = sender.send(1).unwrap_err();
    assert!(matches!(error.error, Error::OutsideVThread));
    assert_eq!(error.value, 1);
    sender.try_send(1).unwrap();
    let error = sender.try_send(2).unwrap_err();
    assert!(matches!(error.error, Error::WouldBlock));
    assert_eq!(error.value, 2);
    assert_eq!(receiver.try_recv().unwrap(), 1);
    assert!(matches!(receiver.try_recv(), Err(Error::WouldBlock)));
}
