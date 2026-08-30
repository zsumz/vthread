//! Bounded MPMC channels with FIFO send/receive waits and explicit disconnection.
//!
//! Blocking operations require a virtual caller. Try operations, cloning, closing,
//! and dropping endpoints work from ordinary threads. Closing or dropping the last
//! sender rejects sends but lets receivers drain buffered values. Dropping the last
//! receiver rejects sends and reclaims buffered values outside the metadata lock.
//! A failed send returns its input; cancellation never consumes a received value.
//!
//! ```
//! use vthread::{Runtime, channel::bounded};
//! let runtime = Runtime::new()?;
//! let (sender, receiver) = bounded(1, 4)?;
//! runtime.scope(|scope| {
//!     let consumer = scope.spawn("consumer", move || receiver.recv())?;
//!     scope.spawn("producer", move || sender.send(42).map_err(|e| e.error))?.join()??;
//!     assert_eq!(consumer.join()??, 42);
//!     Ok(())
//! })?;
//! # Ok::<(), vthread::Error>(())
//! ```

mod core;
mod endpoints;
mod wait;

use crate::{Error, Result, wait::WaitCell};
use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Mutex},
};

/// The sending endpoint of a bounded multiple-producer, multiple-consumer channel.
/// Endpoints cross carriers only when their message type implements `Send`.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread::channel::Sender<std::rc::Rc<usize>>>();
/// ```
pub struct Sender<T> {
    core: Arc<Core<T>>,
}
/// A receiving endpoint; each value is delivered to exactly one receiver.
pub struct Receiver<T> {
    core: Arc<Core<T>>,
}

/// A failed send retains ownership of the original input.
pub struct SendError<T> {
    /// Why the value could not be sent.
    pub error: Error,
    /// The unsent value.
    pub value: T,
}
impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl<T> std::error::Error for SendError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

struct Core<T> {
    capacity: usize,
    wait_capacity: usize,
    state: Mutex<State<T>>,
}
struct State<T> {
    values: VecDeque<T>,
    senders: usize,
    receivers: usize,
    closed: bool,
    send_waits: VecDeque<WaitCell>,
    recv_waits: VecDeque<WaitCell>,
}

/// Creates a channel with positive buffer capacity and a positive waiter limit
/// per direction. Zero-capacity rendezvous channels are not supported.
/// Selected but unconsumed wait tickets still count toward the limit.
pub fn bounded<T>(capacity: usize, wait_capacity: usize) -> Result<(Sender<T>, Receiver<T>)> {
    if capacity == 0 {
        return Err(Error::invalid_configuration(
            "channel_capacity",
            "must be positive",
        ));
    }
    if wait_capacity == 0 {
        return Err(Error::invalid_configuration(
            "wait_capacity",
            "must be positive",
        ));
    }
    let core = Arc::new(Core {
        capacity,
        wait_capacity,
        state: Mutex::new(State {
            values: VecDeque::new(),
            senders: 1,
            receivers: 1,
            closed: false,
            send_waits: VecDeque::new(),
            recv_waits: VecDeque::new(),
        }),
    });
    Ok((
        Sender {
            core: Arc::clone(&core),
        },
        Receiver { core },
    ))
}

#[cfg(test)]
#[path = "channel_test.rs"]
mod channel_test;
