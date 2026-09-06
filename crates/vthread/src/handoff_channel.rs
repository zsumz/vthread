//! Per-carrier channel accounting, including notification coalescing and retries.

use crate::{handoff_span::record, wait::NotifyResult};

/// Endpoint direction; notification counters describe the waiter being notified.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub enum ChannelDirection {
    /// Sending endpoint.
    Send,
    /// Receiving endpoint.
    Receive,
}

/// Counts for carrier-initiated operations after their initial checkpoint.
/// `Stored` counts permit writes, including rewrites of an already stored permit;
/// it is not a count of distinct future resumptions. Closure broadcasts are included.
#[derive(
    ::core::clone::Clone,
    ::core::marker::Copy,
    ::core::fmt::Debug,
    ::core::default::Default,
    ::core::cmp::PartialEq,
    ::core::cmp::Eq,
)]
pub struct ChannelCounters {
    calls: u64,
    transfers: u64,
    failed_calls: u64,
    park_calls: u64,
    park_returns: u64,
    pub(crate) parks: u64,
    retries: u64,
    resumed_transfers: u64,
    resumed_exits: u64,
    selected_notifications: u64,
    stored_notifications: u64,
    closed_notifications: u64,
    ineligible_notifications: u64,
}

impl ChannelCounters {
    /// Calls begun after the blocking entry checkpoint, including try calls.
    pub fn calls(&self) -> u64 {
        self.calls
    }
    /// Values successfully inserted or removed by this direction.
    pub fn transfers(&self) -> u64 {
        self.transfers
    }
    /// Calls dropped without transferring, including error and unwind exits.
    pub fn failed_calls(&self) -> u64 {
        self.failed_calls
    }
    /// Invocations of the notification-park API, including immediate outcomes.
    pub fn park_calls(&self) -> u64 {
        self.park_calls
    }
    /// Successful notification-park API returns, not proof of actual suspension.
    pub fn park_returns(&self) -> u64 {
        self.park_returns
    }
    /// Actual channel-reason park crossings processed by the owner kernel.
    pub fn parks(&self) -> u64 {
        self.parks
    }
    /// Successful park returns followed by another failed resource/turn attempt.
    pub fn retries(&self) -> u64 {
        self.retries
    }
    /// Transfers on the first attempt after a successful park API return.
    pub fn resumed_transfers(&self) -> u64 {
        self.resumed_transfers
    }
    /// Successful park returns followed by an error/unwind without a retry/transfer.
    pub fn resumed_exits(&self) -> u64 {
        self.resumed_exits
    }
    /// Notifications that selected an active wait generation.
    pub fn selected_notifications(&self) -> u64 {
        self.selected_notifications
    }
    /// Notifications that wrote/coalesced a future permit.
    pub fn stored_notifications(&self) -> u64 {
        self.stored_notifications
    }
    /// Notifications rejected by an already-closed wait cell.
    pub fn closed_notifications(&self) -> u64 {
        self.closed_notifications
    }
    /// Notifications while neither a resource nor disconnection was eligible.
    /// Eligibility is observed under the channel lock, not promised to the recipient.
    pub fn ineligible_notifications(&self) -> u64 {
        self.ineligible_notifications
    }
}

pub(crate) struct ChannelCall {
    direction: ChannelDirection,
    returned: bool,
    transferred: bool,
}

impl ChannelCall {
    pub(crate) fn new(direction: ChannelDirection) -> Self {
        record(|profile| profile.channels[direction as usize].calls += 1);
        Self {
            direction,
            returned: false,
            transferred: false,
        }
    }

    pub(crate) fn success(&mut self) {
        record(|profile| {
            let counts = &mut profile.channels[self.direction as usize];
            counts.transfers += 1;
            counts.resumed_transfers += u64::from(self.returned);
        });
        self.returned = false;
        self.transferred = true;
    }

    pub(crate) fn miss(&mut self) {
        if self.returned {
            record(|profile| profile.channels[self.direction as usize].retries += 1);
            self.returned = false;
        }
    }

    pub(crate) fn park(&self) {
        record(|profile| profile.channels[self.direction as usize].park_calls += 1);
    }

    pub(crate) fn returned(&mut self) {
        record(|profile| profile.channels[self.direction as usize].park_returns += 1);
        self.returned = true;
    }
}

impl Drop for ChannelCall {
    fn drop(&mut self) {
        record(|profile| {
            let counts = &mut profile.channels[self.direction as usize];
            counts.failed_calls += u64::from(!self.transferred);
            counts.resumed_exits += u64::from(self.returned);
        });
    }
}

pub(crate) fn notified(direction: ChannelDirection, outcome: NotifyResult, eligible: bool) {
    record(|profile| {
        let counts = &mut profile.channels[direction as usize];
        match outcome {
            NotifyResult::Woke => counts.selected_notifications += 1,
            NotifyResult::Stored => counts.stored_notifications += 1,
            NotifyResult::Closed => counts.closed_notifications += 1,
        }
        counts.ineligible_notifications += u64::from(!eligible);
    });
}

#[cfg(test)]
#[path = "handoff_channel_test.rs"]
mod handoff_channel_test;
