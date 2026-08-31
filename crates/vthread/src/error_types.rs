//! Typed public configuration and result-size failures.

use std::fmt;

/// Result storage that exceeded a caller-selected limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitResource {
    /// Bytes read from a file.
    FileBytes,
    /// Addresses returned by name resolution.
    ResolvedAddresses,
    /// UTF-8 bytes in an operator-visible task name.
    TaskNameBytes,
}

impl fmt::Display for LimitResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FileBytes => "file bytes",
            Self::ResolvedAddresses => "resolved addresses",
            Self::TaskNameBytes => "task name UTF-8 bytes",
        })
    }
}

/// Configuration field rejected before admission or resource creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigurationField {
    /// Runtime readiness registrations.
    IoCapacity,
    /// Native blocking worker count.
    BlockingThreads,
    /// Inactivity observation policy.
    StallPolicy,
    /// Runtime task-record capacity.
    MaxVthreads,
    /// Affine carrier count.
    Carriers,
    /// Unstarted work queue capacity per carrier.
    CarrierQueueCapacity,
    /// Requested virtual-thread stack size.
    StackSize,
    /// Retained stacks per carrier.
    StackCacheCapacity,
    /// Operator-visible task name.
    TaskName,
    /// Maximum resolved address count.
    AddressLimit,
    /// Channel buffer slots.
    ChannelCapacity,
    /// Channel waiter slots.
    ChannelWaitCapacity,
    /// Primitive waiter slots.
    WaitCapacity,
    /// Semaphore permits.
    Permits,
}

impl fmt::Display for ConfigurationField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::IoCapacity => "io_capacity",
            Self::BlockingThreads => "blocking_threads",
            Self::StallPolicy => "stall_policy",
            Self::MaxVthreads => "max_vthreads",
            Self::Carriers => "carriers",
            Self::CarrierQueueCapacity => "carrier_queue_capacity",
            Self::StackSize => "stack_size",
            Self::StackCacheCapacity => "stack_cache_capacity",
            Self::TaskName => "task name",
            Self::AddressLimit => "address_limit",
            Self::ChannelCapacity => "channel capacity",
            Self::ChannelWaitCapacity => "channel wait capacity",
            Self::WaitCapacity => "wait_capacity",
            Self::Permits => "permits",
        })
    }
}

#[cfg(test)]
#[path = "error_types_test.rs"]
mod error_types_test;
