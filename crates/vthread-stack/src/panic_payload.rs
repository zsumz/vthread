//! Shared panic disposal. Never unwind while dropping a caught secondary payload.

use std::{
    any::Any,
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Mutex,
};

/// Maximum retained UTF-8 bytes in one panic message.
pub const MESSAGE_LIMIT: usize = 1024;
/// Process-wide bound on opaque secondary panic payloads retained until process exit.
pub const QUARANTINE_LIMIT: usize = 256;
type Payload = Box<dyn Any + Send>;
static QUARANTINE: Mutex<Vec<Payload>> = Mutex::new(Vec::new());

thread_local! {
    static OBSERVER: Cell<Option<fn(&CapturedPanic)>> = const { Cell::new(None) };
}

/// Installs the runtime's non-panicking cleanup-failure reporter on its owned worker.
pub fn set_cleanup_observer(observer: fn(&CapturedPanic)) {
    OBSERVER.with(|slot| slot.set(Some(observer)));
}

/// Owned, bounded metadata with no user-defined destructor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedPanic {
    pub message: String,
    pub truncated: bool,
    pub cleanup_panicked: bool,
}

/// Captures text before disposing of the original payload under a second boundary.
///
/// Opaque secondary payloads cannot safely be dropped. They are retained in a bounded
/// process-owned quarantine. Exhausting that budget aborts rather than executing an
/// unknown destructor or leaking without a bound. Callers must fail the affected owner
/// when `cleanup_panicked` is true. Arbitrary user double panics and panic=abort remain
/// Rust process-fatal conditions, as do allocation failure and a panicking panic hook.
pub fn capture(payload: Payload) -> CapturedPanic {
    if payload.is::<CapturedPanic>() {
        return *payload
            .downcast::<CapturedPanic>()
            .expect("captured panic type");
    }
    let report = capture_without_observer(payload);
    if report.cleanup_panicked
        && let Ok(Some(observer)) = OBSERVER.try_with(Cell::get)
    {
        observer(&report);
    }
    report
}

/// Captures a joined thread's payload for an explicitly identified failure owner.
pub fn capture_without_observer(payload: Payload) -> CapturedPanic {
    if payload.is::<CapturedPanic>() {
        return *payload
            .downcast::<CapturedPanic>()
            .expect("captured panic type");
    }
    let text = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
        .unwrap_or("non-string panic payload");
    let mut end = text.len().min(MESSAGE_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut report = CapturedPanic {
        message: text[..end].to_owned(),
        truncated: end < text.len(),
        cleanup_panicked: false,
    };
    if let Err(secondary) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
        report.cleanup_panicked = true;
        if secondary.is::<String>() || secondary.is::<&'static str>() {
            // These exact standard types contain no user-defined destructor.
            drop(secondary);
        } else {
            let mut quarantine = QUARANTINE
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if quarantine.len() == QUARANTINE_LIMIT {
                std::process::abort();
            }
            quarantine.push(secondary);
        }
    }
    report
}

pub(crate) fn retain(first: &mut Option<CapturedPanic>, payload: Payload) {
    let next = capture(payload);
    if let Some(first) = first {
        first.cleanup_panicked |= next.cleanup_panicked;
    } else {
        *first = Some(next);
    }
}

pub(crate) fn dispose<T>(value: T, failure: &mut Option<CapturedPanic>) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(value))) {
        retain(failure, payload);
    }
}

#[cfg(test)]
#[path = "panic_payload_test.rs"]
mod panic_payload_test;
