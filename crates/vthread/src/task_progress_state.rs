//! Compact encodings for atomically published task suspension diagnostics.

use crate::{SuspensionReason, TaskId, WakeReason};

pub(super) const RECORD: u8 = 0;
pub(super) const READY: u8 = 1;

pub(super) fn encode_reason(reason: SuspensionReason) -> (u8, u64) {
    match reason {
        SuspensionReason::YieldNow => (2, 0),
        SuspensionReason::Join(task) => (3, task.get()),
        SuspensionReason::ScopeDrain => (4, 0),
        SuspensionReason::Park => (5, 0),
        SuspensionReason::Mutex => (6, 0),
        SuspensionReason::Condvar => (7, 0),
        SuspensionReason::Semaphore => (8, 0),
        SuspensionReason::Notify => (9, 0),
        SuspensionReason::ChannelSend => (10, 0),
        SuspensionReason::ChannelRecv => (11, 0),
        SuspensionReason::IoRead => (12, 0),
        SuspensionReason::IoWrite => (13, 0),
        SuspensionReason::IoAccept => (14, 0),
        SuspensionReason::IoConnect => (15, 0),
        SuspensionReason::Blocking => (16, 0),
        SuspensionReason::Dns => (17, 0),
        SuspensionReason::FileIo => (18, 0),
    }
}

pub(super) fn decode_reason(code: u8, task: u64) -> Option<SuspensionReason> {
    Some(match code {
        2 => SuspensionReason::YieldNow,
        3 => SuspensionReason::Join(TaskId::new(task)),
        4 => SuspensionReason::ScopeDrain,
        5 => SuspensionReason::Park,
        6 => SuspensionReason::Mutex,
        7 => SuspensionReason::Condvar,
        8 => SuspensionReason::Semaphore,
        9 => SuspensionReason::Notify,
        10 => SuspensionReason::ChannelSend,
        11 => SuspensionReason::ChannelRecv,
        12 => SuspensionReason::IoRead,
        13 => SuspensionReason::IoWrite,
        14 => SuspensionReason::IoAccept,
        15 => SuspensionReason::IoConnect,
        16 => SuspensionReason::Blocking,
        17 => SuspensionReason::Dns,
        18 => SuspensionReason::FileIo,
        _ => return None,
    })
}

pub(super) const fn encode_wake(reason: WakeReason) -> u8 {
    match reason {
        WakeReason::Ready => 1,
        WakeReason::TimedOut => 2,
        WakeReason::Cancelled => 3,
        WakeReason::Closed => 4,
    }
}

pub(super) const fn decode_wake(code: u8) -> Option<WakeReason> {
    match code {
        1 => Some(WakeReason::Ready),
        2 => Some(WakeReason::TimedOut),
        3 => Some(WakeReason::Cancelled),
        4 => Some(WakeReason::Closed),
        _ => None,
    }
}

#[cfg(test)]
#[path = "task_progress_state_test.rs"]
mod task_progress_state_test;
