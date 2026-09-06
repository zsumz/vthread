//! Unrelated work must progress while publication remains held. The observer
//! releases a broken spinning implementation only to let its assertion fail safely.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crate::{
    CarrierId, Error, JoinHandle, Runtime,
    control::Shared,
    kernel::Kernel,
    wait::wait_publication_probe_test::{PausedPublication, Stage},
};

pub(super) fn shared() -> Arc<Shared> {
    let config = Runtime::builder()
        .max_vthreads(4)
        .carrier_queue_capacity(4)
        .stack_cache_capacity(4)
        .build()
        .unwrap()
        .config();
    Arc::new(Shared::new(config))
}

pub(super) fn unrelated(shared: &Arc<Shared>, scope: u64) -> Arc<AtomicUsize> {
    let progress = Arc::new(AtomicUsize::new(0));
    let ran = Arc::clone(&progress);
    shared
        .submit(scope, "unrelated runnable work".into(), move || {
            let owner = thread::current().id();
            ran.fetch_add(1, Ordering::Relaxed);
            crate::yield_now().unwrap();
            assert_eq!(thread::current().id(), owner);
            ran.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
    progress
}

pub(super) fn observe_progress(
    mut pause: PausedPublication,
    owner_returned: mpsc::Receiver<()>,
) -> thread::ThreadId {
    let first = pause.next_observation();
    match first.stage {
        Stage::OwnerDeferred => {
            // The owner sends only after checking progress and retained state.
            // Disconnection releases a publisher if an assertion unwinds.
            let _ = owner_returned.recv_timeout(Duration::from_secs(5));
        }
        Stage::FinishWaiting => {
            let repeated = pause.observe(Stage::FinishSpinning);
            assert_eq!(first.token, repeated.token);
            assert_eq!(first.thread, repeated.thread);
            assert!(repeated.elapsed >= first.elapsed);
        }
        Stage::RetireWaiting => {}
        stage => panic!("unexpected owner publication stage: {stage:?}"),
    }
    pause.release();
    first.thread
}

#[test]
fn a_paused_ready_publisher_does_not_occupy_the_recipient_carrier() {
    let shared = shared();
    let scope = shared.begin_scope().unwrap();
    let (parker, waker) = crate::park_pair();
    let cell = parker.wait.clone();
    shared
        .submit(scope, "parked recipient".into(), move || {
            let owner = thread::current().id();
            assert_eq!(parker.park().unwrap(), crate::ParkOutcome::Ready);
            assert_eq!(thread::current().id(), owner);
        })
        .unwrap();
    let progress = unrelated(&shared, scope);
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(false).unwrap());
    assert!(kernel.tick(false).unwrap());
    assert_eq!(kernel.stats.parks, 1);
    assert_eq!(kernel.ready.len(), 1);
    assert_eq!(progress.load(Ordering::Relaxed), 1);
    thread::scope(|threads| {
        let pause = PausedPublication::install(&cell);
        let publisher = threads.spawn(|| waker.unpark());
        let published = pause.observe(Stage::NoticePublished);
        assert_ne!(published.thread, thread::current().id());
        assert_eq!(kernel.inbox.hub.pending(), 1);
        let (returned, resumed) = mpsc::channel();
        let observer = threads.spawn(move || observe_progress(pause, resumed));
        assert!(kernel.tick(false).unwrap());
        assert_eq!(progress.load(Ordering::Relaxed), 2);
        assert!(cell.publication_is_held());
        assert_eq!(kernel.stats.wakes, 0);
        assert_eq!(kernel.parked.len(), 1);
        returned.send(()).unwrap();
        assert_eq!(observer.join().unwrap(), thread::current().id());
        assert_eq!(publisher.join().unwrap(), crate::UnparkResult::Woke);
    });
    assert!(kernel.tick(true).unwrap());
    while kernel.tick(false).unwrap() {}
    assert_eq!(progress.load(Ordering::Relaxed), 2);
    assert_eq!(kernel.stats.wakes, 1);
    assert_eq!(shared.snapshot().active, 0);
    shared.finish_scope(scope);
}

#[test]
fn a_paused_mutex_publisher_preserves_ownership_even_with_selected_cancellation() {
    for cancelled in [false, true] {
        let shared = shared();
        let scope = shared.begin_scope().unwrap();
        let mutex = Arc::new(crate::sync::Mutex::with_wait_capacity(0_usize, 1).unwrap());
        let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
        thread::scope(|threads| {
            let (held_tx, held_rx) = mpsc::channel();
            let (unlock_tx, unlock_rx) = mpsc::channel();
            let publishing = Arc::clone(&mutex);
            let publisher = threads.spawn(move || {
                let guard = publishing.try_lock().unwrap();
                let _ = held_tx.send(());
                let _ = unlock_rx.recv();
                drop(guard);
            });
            held_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let (cell_tx, cell_rx) = mpsc::channel();
            let waiting = Arc::clone(&mutex);
            let spawned = shared
                .submit(scope, "mutex recipient".into(), move || {
                    let owner = thread::current().id();
                    let mounted = crate::context::current().unwrap();
                    let execution = mounted.execution().unwrap();
                    cell_tx
                        .send(execution.synchronization_wait().unwrap().clone())
                        .unwrap();
                    let result = waiting.lock().map(|mut value| *value += 1);
                    assert_eq!(thread::current().id(), owner);
                    result
                })
                .unwrap();
            let mut child = JoinHandle::new(
                Arc::clone(&shared),
                spawned.id,
                spawned.cell,
                spawned.record,
            );
            let progress = unrelated(&shared, scope);
            kernel.receive();
            assert!(kernel.tick(false).unwrap());
            assert!(kernel.tick(false).unwrap());
            assert_eq!(kernel.stats.parks, 1);
            assert_eq!(kernel.ready.len(), 1);
            let cell = cell_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let pause = PausedPublication::install(&cell);
            unlock_tx.send(()).unwrap();
            pause.observe(Stage::NoticePublished);
            assert_eq!(mutex.waiting(), 1, "selected ownership remains accounted");
            assert!(matches!(mutex.try_lock(), Err(Error::WouldBlock)));
            if cancelled {
                child.cancel();
            }
            let (returned, resumed) = mpsc::channel();
            let observer = threads.spawn(move || observe_progress(pause, resumed));
            assert!(kernel.tick(false).unwrap());
            assert_eq!(progress.load(Ordering::Relaxed), 2);
            assert!(cell.publication_is_held());
            assert_eq!(mutex.waiting(), 1);
            assert!(matches!(mutex.try_lock(), Err(Error::WouldBlock)));
            returned.send(()).unwrap();
            assert_eq!(observer.join().unwrap(), thread::current().id());
            publisher.join().unwrap();
            assert!(kernel.tick(true).unwrap());
            while kernel.tick(false).unwrap() {}
            let result = child.take_result().unwrap();
            if cancelled {
                assert!(matches!(result, Err(Error::Cancelled)));
            } else {
                result.unwrap();
            }
            assert_eq!(progress.load(Ordering::Relaxed), 2);
            assert_eq!(mutex.waiting(), 0);
            assert_eq!(*mutex.try_lock().unwrap(), usize::from(!cancelled));
            assert_eq!(shared.snapshot().active, 0);
        });
        shared.finish_scope(scope);
    }
}
