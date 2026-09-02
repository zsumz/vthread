//! Nonblocking producer side of the bounded runtime evidence stream.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc::{SyncSender, TrySendError},
};

use super::stream::{EvidenceStream, NO_DROPPED_SEQUENCE, Status};
use super::{RuntimeEvent, RuntimeEventKind};

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
                self.status.record_drop(sequence);
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
#[path = "recorder_test.rs"]
mod recorder_test;
