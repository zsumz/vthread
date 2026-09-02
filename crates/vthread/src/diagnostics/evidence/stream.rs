//! Bounded single-consumer access to runtime evidence.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    },
    time::Duration,
};

use super::{EventSequence, EvidenceCapabilities, RuntimeEvent, SCHEMA_VERSION};

pub(super) const NO_DROPPED_SEQUENCE: u64 = u64::MAX;

/// Why a blocking evidence receive returned without a batch.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
#[non_exhaustive]
pub enum EvidenceRecvError {
    /// No event arrived before the requested timeout.
    Timeout,
    /// Every evidence producer was dropped and no buffered event remains.
    Disconnected,
}

impl fmt::Display for EvidenceRecvError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Timeout => "runtime evidence receive timed out",
            Self::Disconnected => "runtime evidence stream disconnected",
        })
    }
}

impl std::error::Error for EvidenceRecvError {}

/// Weakly consistent evidence-buffer counters.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct EvidenceStatus {
    capacity: usize,
    pending: usize,
    recorded: u64,
    dropped: u64,
    first_dropped: Option<EventSequence>,
    runtime_terminal: bool,
}

impl EvidenceStatus {
    /// Configured maximum number of undrained events.
    pub fn capacity(self) -> usize {
        self.capacity
    }

    /// Events currently waiting for the consumer.
    pub fn pending(self) -> usize {
        self.pending
    }

    /// Events successfully admitted to the evidence buffer.
    pub fn recorded(self) -> u64 {
        self.recorded
    }

    /// Events rejected because the buffer was full or disconnected.
    pub fn dropped(self) -> u64 {
        self.dropped
    }

    /// First sequence that could not be recorded.
    pub fn first_dropped(self) -> Option<EventSequence> {
        self.first_dropped
    }

    /// Whether runtime shutdown reached a terminal phase.
    pub fn runtime_terminal(self) -> bool {
        self.runtime_terminal
    }

    /// Whether the evidence admitted so far has no known loss.
    pub fn is_complete(self) -> bool {
        self.dropped == 0
    }
}

pub(super) struct Status {
    pub(super) capacity: usize,
    pub(super) next_sequence: AtomicU64,
    pub(super) pending: AtomicUsize,
    pub(super) recorded: AtomicU64,
    pub(super) dropped: AtomicU64,
    pub(super) first_dropped: AtomicU64,
    pub(super) runtime_terminal: AtomicBool,
}

impl Status {
    fn snapshot(&self) -> EvidenceStatus {
        let first = self.first_dropped.load(Ordering::Acquire);
        EvidenceStatus {
            capacity: self.capacity,
            pending: self.pending.load(Ordering::Acquire),
            recorded: self.recorded.load(Ordering::Acquire),
            dropped: self.dropped.load(Ordering::Acquire),
            first_dropped: (first != NO_DROPPED_SEQUENCE).then(|| EventSequence::new(first)),
            runtime_terminal: self.runtime_terminal.load(Ordering::Acquire),
        }
    }

    pub(super) fn record_drop(&self, sequence: u64) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.first_dropped.compare_exchange(
            NO_DROPPED_SEQUENCE,
            sequence,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }
}

/// Single-consumer handle for receiving sequenced runtime evidence.
pub struct EvidenceStream {
    pub(super) receiver: Receiver<RuntimeEvent>,
    pub(super) status: Arc<Status>,
}

impl EvidenceStream {
    /// Returns the independent evidence schema version.
    pub fn schema_version(&self) -> u16 {
        SCHEMA_VERSION
    }

    /// Returns the exact evidence capabilities compiled into this stream.
    pub fn capabilities(&self) -> EvidenceCapabilities {
        EvidenceCapabilities::runtime()
    }

    /// Drains one bounded batch and orders it by sequence.
    /// Consumers merging concurrent batches must continue to use sequence values.
    pub fn drain(&mut self) -> Vec<RuntimeEvent> {
        self.collect_batch(None)
    }

    /// Waits for one event, then collects one bounded, sequence-ordered batch.
    ///
    /// The timeout applies only to the first event. Once one arrives, this method collects
    /// immediately available events up to the configured evidence capacity. This is a blocking
    /// standard-library wait intended for an ordinary OS or monitoring thread, not a carrier.
    pub fn recv_batch_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<RuntimeEvent>, EvidenceRecvError> {
        let first = match self.receiver.recv_timeout(timeout) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => return Err(EvidenceRecvError::Timeout),
            Err(RecvTimeoutError::Disconnected) => return Err(EvidenceRecvError::Disconnected),
        };
        Ok(self.collect_batch(Some(first)))
    }

    /// Returns current buffer and loss counters.
    pub fn status(&self) -> EvidenceStatus {
        self.status.snapshot()
    }

    fn collect_batch(&mut self, first: Option<RuntimeEvent>) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        if let Some(first) = first {
            events.push(first);
        }
        while events.len() < self.status.capacity {
            match self.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        self.status
            .pending
            .fetch_sub(events.len(), Ordering::AcqRel);
        events.sort_unstable_by_key(|event| event.sequence());
        events
    }
}

#[cfg(test)]
#[path = "stream_test.rs"]
mod stream_test;
