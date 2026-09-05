//! Execute the production mailbox and its sibling tests with modeled atomics.

#![forbid(unsafe_code)]

mod wake_atomic {
    pub(crate) use loom::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        },
        thread,
    };

    pub(crate) fn model(f: impl Fn() + Send + Sync + 'static) {
        let mut builder = loom::model::Builder::new();
        builder.max_threads = 3;
        builder.max_branches = 1_000;
        // These finite tests must finish exploration, not silently stop at a budget.
        builder.max_permutations = None;
        builder.max_duration = None;
        builder.preemption_bound = None;
        builder.checkpoint_file = None;
        builder.check(f);
    }
}

#[path = "../src/wake_mailbox.rs"]
mod wake_mailbox;

#[path = "../src/wake_inbox.rs"]
mod wake_inbox;

pub(crate) use wake_inbox::{WakeInbox, WakePacket};
pub use wake_mailbox::WakeMailbox;

#[path = "support/wake_inbox_signal_test.rs"]
mod wake_inbox_signal_test;
