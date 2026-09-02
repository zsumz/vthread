//! Task panic isolation includes storing and dropping an unjoined result.

use crate::{PanicReport, task::SharedTaskRecord};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(crate) fn run<T>(
    record: &SharedTaskRecord,
    entry: impl FnOnce() -> T,
    store: impl FnOnce(std::result::Result<T, PanicReport>),
) {
    let outcome = catch_unwind(AssertUnwindSafe(entry)).map_err(PanicReport::capture);
    if let Err(panic) = &outcome {
        record.lock().panic = Some(panic.clone());
    }
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| store(outcome))) {
        record.lock().panic = Some(PanicReport::capture(payload));
    }
}

#[cfg(test)]
#[path = "task_body_test.rs"]
mod task_body_test;
