//! Ordered control is outside acquisition samples, but included in process costs.

use crate::{
    mutex_handoff::{Config, Mode, check_owners},
    mutex_handoff_linux as linux,
};
use std::{
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub(crate) struct Shared {
    config: Config,
    pub mutex: vthread::sync::Mutex<usize>,
    wake: vthread::parking::Unparker,
    requested: AtomicUsize,
    attempting: AtomicUsize,
    parked: AtomicUsize,
    completed: AtomicUsize,
    recipient_tid: AtomicUsize,
    stop: AtomicBool,
    keeper_done: AtomicBool,
    stamp: AtomicU64,
    origin: Instant,
}

struct Stop<'a>(&'a Shared);
impl Drop for Stop<'_> {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.wake.unpark();
    }
}

impl Shared {
    pub(crate) fn new(config: Config, wake: vthread::parking::Unparker) -> Self {
        Self {
            config,
            mutex: vthread::sync::Mutex::with_wait_capacity(0, 1).unwrap(),
            wake,
            requested: AtomicUsize::new(0),
            attempting: AtomicUsize::new(0),
            parked: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            recipient_tid: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            keeper_done: AtomicBool::new(false),
            stamp: AtomicU64::new(0),
            origin: Instant::now(),
        }
    }

    fn tick(&self) -> vthread::Result<()> {
        if self.stop.load(Ordering::Acquire) || self.origin.elapsed() > Duration::from_secs(60) {
            return Err(linux::error(
                "controlled handoff stopped or exceeded its watchdog",
            ));
        }
        vthread::yield_now()
    }

    fn now(&self) -> u64 {
        self.origin
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX - 1)) as u64
            + 1
    }
}

pub(crate) fn sender(shared: &Shared) -> vthread::Result<(usize, usize, usize)> {
    let _stop = Stop(shared);
    let tid = linux::tid()?;
    while shared.recipient_tid.load(Ordering::Acquire) == 0 {
        shared.tick()?;
    }
    let recipient = shared.recipient_tid.load(Ordering::Acquire);
    check_owners(shared.config.mode, tid, recipient)?;
    let mut sleeps = 0;
    for generation in 1..=shared.config.iterations {
        let mut guard = shared.mutex.lock()?;
        *guard = generation;
        shared.requested.store(generation, Ordering::Release);
        shared.wake.unpark();
        // In local mode the source cannot run while the recipient is inside
        // lock(): observing its ticket here therefore follows its suspension.
        while shared.mutex.waiting() != 1 {
            shared.tick()?;
        }
        match shared.config.mode {
            Mode::Local => {}
            Mode::RemoteActive => {
                // Only a runnable child on the actual recipient owner writes
                // this marker, after the recipient enters the held mutex.
                while shared.parked.load(Ordering::Acquire) != generation {
                    shared.tick()?;
                }
            }
            Mode::RemoteSleepObserved => {
                while !linux::sleeping(recipient)? {
                    shared.tick()?;
                }
                sleeps += 1;
            }
        }
        if shared.mutex.waiting() != 1 || shared.attempting.load(Ordering::Acquire) != generation {
            return Err(linux::error(
                "recipient was not queued before measured release",
            ));
        }
        shared.stamp.store(shared.now(), Ordering::Release);
        drop(guard);
        while shared.completed.load(Ordering::Acquire) != generation {
            shared.tick()?;
        }
        if linux::tid()? != tid {
            return Err(linux::error("source migrated"));
        }
    }
    Ok((tid, shared.config.iterations, sleeps))
}

pub(crate) fn recipient(
    shared: &Shared,
    park: vthread::parking::Parker,
) -> vthread::Result<(usize, Vec<u64>, u64)> {
    let _stop = Stop(shared);
    let tid = linux::tid()?;
    shared.recipient_tid.store(tid, Ordering::Release);
    if shared.config.mode == Mode::RemoteActive {
        vthread::local_scope(|scope| {
            let mut keeper = scope.spawn("mutex-active-keeper", || keeper(shared, tid))?;
            let samples = acquire(shared, &park, tid);
            shared.keeper_done.store(true, Ordering::Release);
            let yields = keeper.join()??;
            Ok((tid, samples?, yields))
        })
    } else {
        Ok((tid, acquire(shared, &park, tid)?, 0))
    }
}

fn keeper(shared: &Shared, owner: usize) -> vthread::Result<u64> {
    if linux::tid()? != owner {
        return Err(linux::error("active keeper is on a different owner"));
    }
    let mut yields = 0;
    while !shared.keeper_done.load(Ordering::Acquire) && !shared.stop.load(Ordering::Acquire) {
        let generation = shared.attempting.load(Ordering::Acquire);
        if shared.mutex.waiting() == 1 {
            shared.parked.store(generation, Ordering::Release);
        }
        if let Err(error) = shared.tick() {
            if shared.keeper_done.load(Ordering::Acquire) || shared.stop.load(Ordering::Acquire) {
                break;
            }
            return Err(error);
        }
        yields += 1;
    }
    Ok(yields)
}

fn acquire(
    shared: &Shared,
    park: &vthread::parking::Parker,
    owner: usize,
) -> vthread::Result<Vec<u64>> {
    let mut samples = Vec::with_capacity(shared.config.iterations);
    for generation in 1..=shared.config.iterations {
        // A separate command park prevents accidental uncontended reacquisition.
        // Its crossings and wake routing remain visible in whole-process totals.
        park.park()?;
        if shared.stop.load(Ordering::Acquire) {
            return Err(linux::error("source stopped"));
        }
        if shared.requested.load(Ordering::Acquire) != generation {
            return Err(linux::error("command generation mismatch"));
        }
        shared.attempting.store(generation, Ordering::Release);
        let value = shared.mutex.lock()?;
        let finished = shared.now();
        let started = shared.stamp.swap(0, Ordering::Acquire);
        if started == 0 || started > finished || *value != generation {
            return Err(linux::error("stale acquisition sample or payload"));
        }
        samples.push(finished - started);
        drop(value);
        if linux::tid()? != owner {
            return Err(linux::error("recipient migrated"));
        }
        shared.completed.store(generation, Ordering::Release);
    }
    Ok(samples)
}

#[cfg(test)]
#[path = "mutex_handoff_tasks_test.rs"]
mod mutex_handoff_tasks_test;
