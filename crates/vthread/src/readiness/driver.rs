//! The poll owner alone mutates registrations; readiness always selects an exact park token.

use super::{Inner, State, io_error};
use crate::signal::lock;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

pub(super) fn start(
    capacity: usize,
    ready: std::sync::mpsc::SyncSender<crate::Result<Arc<Inner>>>,
    owner: std::sync::Weak<crate::control::Shared>,
) {
    let initialized = initialize(capacity, owner);
    match initialized {
        Ok((inner, poll, events)) => {
            if ready.send(Ok(Arc::clone(&inner))).is_ok() {
                run(inner, poll, events);
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

fn initialize(
    capacity: usize,
    owner: std::sync::Weak<crate::control::Shared>,
) -> crate::Result<(Arc<Inner>, zio::Poll, zio::Events)> {
    let mut poll = zio::Poll::with_capacity(capacity.max(2), capacity).map_err(io_error)?;
    let events = poll.events().map_err(io_error)?;
    let waker = poll.waker(zio::Key::ZERO).map_err(io_error)?;
    let inner = Arc::new(Inner {
        owner,
        state: std::sync::Mutex::new(State {
            entries: BTreeMap::new(),
            next: 1,
            stopped: false,
            error: None,
        }),
        waker,
        capacity,
        registered: std::sync::atomic::AtomicUsize::new(0),
        #[cfg(test)]
        fail_wait: std::sync::atomic::AtomicBool::new(false),
    });
    Ok((inner, poll, events))
}

fn run(inner: Arc<Inner>, mut poll: zio::Poll, mut events: zio::Events) {
    let outcome = catch_unwind(AssertUnwindSafe(|| drive(&inner, &mut poll, &mut events)));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => inner.close(Some(error)),
        Err(payload) => inner.close(Some(
            crate::PanicReport::capture(payload).message().to_owned(),
        )),
    }
    drop(poll);
    inner.registered.store(0, Ordering::Release);
}

fn drive(inner: &Inner, poll: &mut zio::Poll, events: &mut zio::Events) -> Result<(), String> {
    let mut installed = BTreeMap::new();
    loop {
        {
            let state = lock(&inner.state);
            if state.stopped {
                return Ok(());
            }
            let removed = installed
                .keys()
                .filter(|key| !state.entries.contains_key(key))
                .copied()
                .collect::<Vec<_>>();
            for key in removed {
                poll.delete(installed.remove(&key).expect("installed key"))
                    .map_err(|error| error.to_string())?;
            }
            for (&key, entry) in &state.entries {
                if let std::collections::btree_map::Entry::Vacant(slot) = installed.entry(key) {
                    let registration = poll
                        .register(
                            &entry.fd,
                            zio::Key::new(key),
                            entry.interest,
                            zio::Mode::Level,
                        )
                        .map_err(|error| error.to_string())?;
                    slot.insert(registration);
                }
            }
            inner.registered.store(installed.len(), Ordering::Release);
        }
        // Native waiting never holds the shared metadata mutex.
        #[cfg(test)]
        if inner.fail_wait.swap(false, Ordering::AcqRel) {
            return Err("injected readiness wait failure".to_owned());
        }
        let report = match poll.wait(events, zio::Wait::For(Duration::from_millis(100))) {
            Ok(report) => report,
            Err(zio::Error::Io { source, .. })
                if source.kind() == std::io::ErrorKind::Interrupted =>
            {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        for event in events.iter() {
            if event.readiness().is_none() {
                continue;
            }
            let entry = lock(&inner.state).entries.remove(&event.key().get());
            if let Some(entry) = entry {
                entry.wake.select_ready(entry.token);
            }
        }
        if let Some(error) = report.into_recovery() {
            return Err(error.to_string());
        }
    }
}

#[cfg(test)]
#[path = "driver_test.rs"]
mod driver_test;
