//! Suspension, resumption, and outcome values shared by every stack engine.

use std::{error::Error, fmt, time::Instant};

/// Identity for one generation of a parking operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParkToken {
    wait: u64,
    generation: u64,
}

impl ParkToken {
    /// Creates a scheduler-visible token.
    pub fn new(wait: u64, generation: u64) -> Self {
        Self { wait, generation }
    }

    /// Returns the stable parking-object identity.
    pub fn wait(self) -> u64 {
        self.wait
    }

    /// Returns the monotonically increasing wait generation.
    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// Scheduler data supplied when a task parks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParkRequest {
    token: ParkToken,
    deadline: Option<Instant>,
}

impl ParkRequest {
    /// Creates a parking request with an optional monotonic deadline.
    pub fn new(token: ParkToken, deadline: Option<Instant>) -> Self {
        Self { token, deadline }
    }

    /// Returns the wait token.
    pub fn token(&self) -> ParkToken {
        self.token
    }

    /// Returns the monotonic deadline, if one exists.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

/// A reason a mounted fiber returned control to its carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Suspension {
    /// The virtual thread cooperatively yielded its turn.
    YieldNow,
    /// The virtual thread parked on a modeled wait generation.
    Park(ParkRequest),
}

/// The outcome of mounting a fiber once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FiberState {
    /// Execution suspended with the supplied reason.
    Suspended(Suspension),
    /// The fiber returned from its entry function.
    Complete,
}

/// A carrier decision delivered to the operation that suspended the fiber.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Resume {
    /// Continue the suspended operation normally.
    #[default]
    Continue,
    /// Recheck runtime policy before the suspended operation returns.
    Interrupt,
}

/// Suspension was requested without a mounted fiber.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendError;

impl fmt::Display for SuspendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("no virtual-thread stack is mounted on this carrier")
    }
}

impl Error for SuspendError {}

#[cfg(test)]
#[path = "suspension_test.rs"]
mod suspension_test;
