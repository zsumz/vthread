use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const EMPTY: u64 = 0;

#[derive(Clone, Copy)]
pub(crate) struct WakeClock {
    origin: Instant,
}

#[repr(align(64))]
pub(crate) struct WakeStamp {
    encoded_ns: AtomicU64,
}

impl WakeClock {
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }

    pub(crate) fn publish(self, stamp: &WakeStamp) {
        stamp
            .encoded_ns
            .store(self.encoded_now(), Ordering::Release);
    }

    pub(crate) fn elapsed(self, stamp: &WakeStamp) -> u64 {
        let published = stamp.encoded_ns.load(Ordering::Acquire);
        assert_ne!(published, EMPTY, "wake timestamp must be published");
        let observed = self.encoded_now();
        stamp.encoded_ns.store(EMPTY, Ordering::Relaxed);
        observed.saturating_sub(published)
    }

    fn encoded_now(self) -> u64 {
        encode_nanoseconds(self.origin.elapsed().as_nanos())
    }
}

impl WakeStamp {
    pub(crate) fn new() -> Self {
        Self {
            encoded_ns: AtomicU64::new(EMPTY),
        }
    }
}

fn encode_nanoseconds(nanoseconds: u128) -> u64 {
    nanoseconds.min(u128::from(u64::MAX - 1)) as u64 + 1
}

#[cfg(test)]
#[path = "wake_clock_test.rs"]
mod wake_clock_test;
