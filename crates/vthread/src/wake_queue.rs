//! Fixed-capacity MPSC wake routing with one owner-carrier consumer.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use vthread_stack::ParkToken;

use crate::{
    TaskId,
    task_slab::TaskKey,
    wait::{WakeCause, WakeNotice},
};

#[path = "wake_queue_core.rs"]
mod wake_queue_core;
pub(crate) use wake_queue_core::WakeQueue;
#[cfg(test)]
use wake_queue_core::{Cursor, Slot};

#[cfg(test)]
#[path = "wake_queue_test.rs"]
mod wake_queue_test;
