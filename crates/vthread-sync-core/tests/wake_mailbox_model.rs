//! Execute the production mailbox and its sibling tests with modeled atomics.

#![forbid(unsafe_code)]

mod wake_atomic {
    pub(crate) use loom::{
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        thread,
    };

    pub(crate) fn model(f: impl Fn() + Send + Sync + 'static) {
        let mut builder = loom::model::Builder::new();
        builder.max_threads = 3;
        builder.max_branches = 200;
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

pub use wake_mailbox::WakeMailbox;
