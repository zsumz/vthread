use super::bounded;
use crate::{Error, Runtime};
use std::sync::{Arc, Mutex};

#[test]
fn mpmc_delivers_each_value_once_on_one_and_four_carriers() {
    for carriers in [1, 4] {
        let runtime = Runtime::builder().carriers(carriers).build().unwrap();
        let (sender, receiver) = bounded(3, 8).unwrap();
        let output = Arc::new(Mutex::new(Vec::new()));
        runtime
            .scope(|scope| {
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
                for task in tasks {
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
fn constructors_and_nonblocking_operations_enforce_bounds() {
    assert!(bounded::<u8>(0, 1).is_err());
    assert!(bounded::<u8>(1, 0).is_err());
    let (sender, receiver) = bounded(1, 1).unwrap();
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
