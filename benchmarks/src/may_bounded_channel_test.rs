use super::bounded;
use std::sync::{Arc, Barrier};

#[test]
fn values_release_capacity_for_the_next_send() {
    let (sender, receiver) = bounded(1);
    sender.send(7).unwrap();
    assert_eq!(receiver.recv().unwrap(), 7);
    sender.send(9).unwrap();
    assert_eq!(receiver.recv().unwrap(), 9);
}

#[test]
fn receiver_drop_releases_a_sender_waiting_on_capacity() {
    let (sender, receiver) = bounded(1);
    sender.send(7).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let sender_barrier = Arc::clone(&barrier);
    let sender = std::thread::spawn(move || {
        sender_barrier.wait();
        sender.send(9)
    });
    barrier.wait();
    drop(receiver);
    assert_eq!(sender.join().unwrap().unwrap_err().0, 9);
}
