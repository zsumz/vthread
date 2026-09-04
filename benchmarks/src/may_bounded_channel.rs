use may::sync::{Semphore, spsc};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{RecvError, SendError},
};

struct Gate {
    free: Semphore,
    receiver_live: AtomicBool,
}

pub(crate) struct Sender<T> {
    inner: spsc::Sender<T>,
    gate: Arc<Gate>,
}

pub(crate) struct Receiver<T> {
    inner: spsc::Receiver<T>,
    gate: Arc<Gate>,
}

pub(crate) fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "bounded benchmark capacity must be positive");
    assert!(capacity < isize::MAX as usize, "capacity must fit isize");
    let (sender, receiver) = spsc::channel();
    let gate = Arc::new(Gate {
        free: Semphore::new(capacity),
        receiver_live: AtomicBool::new(true),
    });
    (
        Sender {
            inner: sender,
            gate: Arc::clone(&gate),
        },
        Receiver {
            inner: receiver,
            gate,
        },
    )
}

impl<T> Sender<T> {
    pub(crate) fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.gate.free.wait();
        if !self.gate.receiver_live.load(Ordering::Acquire) {
            self.gate.free.post();
            return Err(SendError(value));
        }
        self.inner
            .send(value)
            .inspect_err(|_| self.gate.free.post())
    }
}

impl<T> Receiver<T> {
    pub(crate) fn recv(&self) -> Result<T, RecvError> {
        self.inner.recv().inspect(|_| self.gate.free.post())
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.gate.receiver_live.store(false, Ordering::Release);
        self.gate.free.post();
    }
}

#[cfg(test)]
#[path = "may_bounded_channel_test.rs"]
mod may_bounded_channel_test;
