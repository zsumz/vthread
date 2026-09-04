use crate::{SuspensionReason, TaskId, WakeReason};

use super::{decode_reason, decode_wake, encode_reason, encode_wake};

#[test]
fn every_suspension_reason_round_trips_without_losing_join_identity() {
    let reasons = [
        SuspensionReason::YieldNow,
        SuspensionReason::Join(TaskId::new(91)),
        SuspensionReason::ScopeDrain,
        SuspensionReason::Park,
        SuspensionReason::Mutex,
        SuspensionReason::Condvar,
        SuspensionReason::Semaphore,
        SuspensionReason::Notify,
        SuspensionReason::ChannelSend,
        SuspensionReason::ChannelRecv,
        SuspensionReason::IoRead,
        SuspensionReason::IoWrite,
        SuspensionReason::IoAccept,
        SuspensionReason::IoConnect,
        SuspensionReason::Blocking,
        SuspensionReason::Dns,
        SuspensionReason::FileIo,
    ];
    for reason in reasons {
        let (code, task) = encode_reason(reason);
        assert_eq!(decode_reason(code, task), Some(reason));
    }
}

#[test]
fn every_wake_reason_round_trips() {
    for reason in [
        WakeReason::Ready,
        WakeReason::TimedOut,
        WakeReason::Cancelled,
        WakeReason::Closed,
    ] {
        assert_eq!(decode_wake(encode_wake(reason)), Some(reason));
    }
}
