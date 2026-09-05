use std::sync::{Arc, mpsc};

use super::Mutex;
use crate::{CarrierId, Error, JoinHandle, Runtime, control::Shared, kernel::Kernel, signal::lock};

#[test]
fn cancellation_between_dequeue_and_grant_cannot_orphan_mutex_ownership() {
    let mutex = Arc::new(Mutex::with_wait_capacity(0_usize, 1).unwrap());
    let config = Runtime::builder()
        .max_vthreads(64)
        .carrier_queue_capacity(64)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));

    std::thread::scope(|threads| {
        // The sender locals release blocked threads if an assertion unwinds.
        let (held_tx, held_rx) = mpsc::channel();
        let (unlock_tx, unlock_rx) = mpsc::channel();
        let (dequeued_tx, dequeued_rx) = mpsc::channel();
        let (publish_tx, publish_rx) = mpsc::channel();
        *lock(&mutex.queue.after_dequeue) = Some(Box::new(move || {
            let _ = dequeued_tx.send(());
            let _ = publish_rx.recv();
        }));
        let remote = Arc::clone(&mutex);
        let owner = threads.spawn(move || {
            // The affine guard is created and dropped on this same OS thread.
            let guard = remote.try_lock().unwrap();
            let _ = held_tx.send(());
            let _ = unlock_rx.recv();
            drop(guard);
        });
        held_rx.recv().unwrap();
        let waiting = Arc::clone(&mutex);
        let spawned = shared
            .submit(scope, "cancelled mutex recipient".into(), move || {
                waiting.lock().map(drop)
            })
            .unwrap();
        let mut child = JoinHandle::new(
            Arc::clone(&shared),
            spawned.id,
            spawned.cell,
            spawned.record,
        );
        kernel.receive();
        assert!(kernel.tick(false).unwrap());
        assert_eq!(mutex.waiting(), 1);

        unlock_tx.send(()).unwrap();
        dequeued_rx.recv().unwrap();
        child.cancel();
        // Drive cancellation while the dequeued handoff has not been published.
        // A ready claim may already own this generation; do not require cancellation
        // to beat that claim, only that its ownership is ultimately returned.
        kernel.tick(true).unwrap();
        publish_tx.send(()).unwrap();
        owner.join().unwrap();
        for _ in 0..3 {
            kernel.tick(false).unwrap();
        }
        assert!(child.is_finished());
        assert!(matches!(
            child.take_result().unwrap(),
            Err(Error::Cancelled)
        ));
        assert_eq!(mutex.waiting(), 0);
        assert!(
            mutex.try_lock().is_ok(),
            "cancelled recipient orphaned ownership"
        );
    });
    shared.finish_scope(scope);
}
