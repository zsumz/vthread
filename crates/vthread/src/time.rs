//! Monotonic virtual-thread sleeping.

use std::time::{Duration, Instant};

use crate::{Error, ParkOutcome, Result, park_pair, yield_now};

/// Parks the current virtual thread for at least `duration`.
pub fn sleep(duration: Duration) -> Result<()> {
    if duration.is_zero() {
        return yield_now();
    }
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or(Error::DeadlineOverflow)?;
    sleep_until(deadline)
}

/// Parks the current virtual thread until a monotonic deadline.
pub fn sleep_until(deadline: Instant) -> Result<()> {
    if deadline <= Instant::now() {
        return Ok(());
    }
    let (parker, _unparker) = park_pair();
    match parker.park_until(deadline)? {
        ParkOutcome::TimedOut => Ok(()),
        _ => Err(Error::Invariant("private sleep parker woke without timeout")),
    }
}

#[cfg(test)]
#[path = "time_test.rs"]
mod time_test;
