//! Opt-in, bounded runtime evidence for qualification and exact replay.
//!
//! Evidence producers never run consumer code and never wait for consumer capacity.
//! Events from concurrent producers may arrive out of order; their runtime-wide sequence
//! is authoritative. Any full or disconnected buffer is reported as incomplete evidence.

mod emitter;
mod event;
pub(crate) use emitter::Emitter;
pub use event::{
    EventSequence, EvidenceWakeCause, QueueKind, RuntimeEvent, RuntimeEventKind, StackDisposition,
    StackId, TaskOutcome, TimerRetirement, WaitKey, WakeRejection,
};

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, TrySendError},
};

const NO_DROPPED_SEQUENCE: u64 = u64::MAX;

/// Evidence schema emitted by this release candidate.
pub const SCHEMA_VERSION: u16 = 1;

/// Capabilities supported by one evidence stream.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct EvidenceCapabilities(u64);

impl EvidenceCapabilities {
    /// Runtime-wide event sequence numbers.
    pub const TOTAL_ORDER: Self = Self(1 << 0);
    /// Scope open and close transitions.
    pub const SCOPE_LIFECYCLE: Self = Self(1 << 1);
    /// Task admission, mount, suspension and terminal transitions.
    pub const TASK_LIFECYCLE: Self = Self(1 << 2);
    /// Exact reusable wait identities and generations.
    pub const WAIT_GENERATIONS: Self = Self(1 << 3);
    /// Wake offers, sole-winner selection and typed rejection.
    pub const WAKE_SELECTION: Self = Self(1 << 4);
    /// Reusable stack mapping checkout and release identities.
    pub const STACK_IDENTITIES: Self = Self(1 << 5);
    /// Exact timer registration and retirement transitions.
    pub const TIMER_LIFECYCLE: Self = Self(1 << 6);
    /// Bounded admission and wake queue depths.
    pub const QUEUE_DEPTHS: Self = Self(1 << 7);
    /// Runtime shutdown phase transitions.
    pub const SHUTDOWN_LIFECYCLE: Self = Self(1 << 8);
    /// Repeated mounts include the immutable owner carrier.
    pub const CARRIER_AFFINITY: Self = Self(1 << 9);
    /// A generation-bound stale-wake probe is compiled in.
    pub const STALE_WAKE_PROBE: Self = Self(1 << 10);

    fn runtime() -> Self {
        let base = Self::TOTAL_ORDER.0
            | Self::SCOPE_LIFECYCLE.0
            | Self::TASK_LIFECYCLE.0
            | Self::WAIT_GENERATIONS.0
            | Self::WAKE_SELECTION.0
            | Self::STACK_IDENTITIES.0
            | Self::TIMER_LIFECYCLE.0
            | Self::QUEUE_DEPTHS.0
            | Self::SHUTDOWN_LIFECYCLE.0
            | Self::CARRIER_AFFINITY.0;
        #[cfg(feature = "qualification")]
        let base = base | Self::STALE_WAKE_PROBE.0;
        Self(base)
    }

    /// Returns whether every requested capability is present.
    pub fn contains(self, requested: Self) -> bool {
        self.0 & requested.0 == requested.0
    }

    /// Returns the stable bit representation for trace headers.
    pub fn bits(self) -> u64 {
        self.0
    }
}

impl std::ops::BitOr for EvidenceCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for EvidenceCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

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

struct Status {
    capacity: usize,
    next_sequence: AtomicU64,
    pending: AtomicUsize,
    recorded: AtomicU64,
    dropped: AtomicU64,
    first_dropped: AtomicU64,
    runtime_terminal: AtomicBool,
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

    fn dropped(&self, sequence: u64) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.first_dropped.compare_exchange(
            NO_DROPPED_SEQUENCE,
            sequence,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }
}

/// Single-consumer handle for draining sequenced runtime evidence.
pub struct EvidenceStream {
    receiver: Receiver<RuntimeEvent>,
    status: Arc<Status>,
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

    /// Drains the events currently available and orders this batch by sequence.
    /// Consumers merging concurrent batches must continue to use sequence values.
    pub fn drain(&mut self) -> Vec<RuntimeEvent> {
        let mut events = self.receiver.try_iter().collect::<Vec<_>>();
        self.status
            .pending
            .fetch_sub(events.len(), Ordering::AcqRel);
        events.sort_by_key(|event| event.sequence());
        events
    }

    /// Returns current buffer and loss counters.
    pub fn status(&self) -> EvidenceStatus {
        self.status.snapshot()
    }
}

#[derive(::core::clone::Clone)]
pub(crate) struct Recorder {
    sender: SyncSender<RuntimeEvent>,
    status: Arc<Status>,
}

impl Recorder {
    pub(crate) fn record(&self, runtime: crate::diagnostics::RuntimeId, kind: RuntimeEventKind) {
        let sequence = self
            .status
            .next_sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("runtime evidence sequence exhausted");
        let terminal = core::matches!(
            kind,
            RuntimeEventKind::ShutdownAdvanced {
                phase: crate::ShutdownPhase::Complete | crate::ShutdownPhase::Failed
            }
        );
        self.status.pending.fetch_add(1, Ordering::AcqRel);
        match self
            .sender
            .try_send(RuntimeEvent::new(sequence, runtime, kind))
        {
            Ok(()) => {
                self.status.recorded.fetch_add(1, Ordering::Relaxed);
                if terminal {
                    self.status.runtime_terminal.store(true, Ordering::Release);
                }
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.status.pending.fetch_sub(1, Ordering::AcqRel);
                self.status.dropped(sequence);
            }
        }
    }
}

pub(crate) fn bounded(capacity: usize) -> (Recorder, EvidenceStream) {
    let (sender, receiver) = std::sync::mpsc::sync_channel(capacity);
    let status = Arc::new(Status {
        capacity,
        next_sequence: AtomicU64::new(0),
        pending: AtomicUsize::new(0),
        recorded: AtomicU64::new(0),
        dropped: AtomicU64::new(0),
        first_dropped: AtomicU64::new(NO_DROPPED_SEQUENCE),
        runtime_terminal: AtomicBool::new(false),
    });
    (
        Recorder {
            sender,
            status: Arc::clone(&status),
        },
        EvidenceStream { receiver, status },
    )
}

#[cfg(test)]
#[path = "evidence_test.rs"]
mod evidence_test;
