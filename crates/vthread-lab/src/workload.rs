//! Payload-checked channels, TCP, native jobs, timers, and cancellation races.

use std::{sync::Arc, thread, time::Duration};
use vthread::{Result, Runtime, Scope, blocking, channel, parking::park_pair, sleep, yield_now};

pub(crate) const MUTEX_UPDATES_PER_TASK: usize = 8;

pub(crate) fn batch(
    scope: &Scope<'_>,
    tasks: usize,
    iteration: u64,
    previous: Option<super::network::Pair>,
) -> Result<super::network::Pair> {
    let (sender, receiver) = channel::bounded_with_wait_capacity(8, tasks + 8)?;
    let mut producer = scope.spawn("soak-channel-send", move || -> Result<()> {
        for value in 0..128u64 {
            sender.send(value).map_err(|error| error.into_parts().0)?;
        }
        Ok(())
    })?;
    let mut consumer = scope.spawn("soak-channel-recv", move || -> Result<()> {
        for value in 0..128u64 {
            assert_eq!(receiver.recv()?, value);
        }
        Ok(())
    })?;
    let exchange = super::network::start(scope, iteration, previous)?;
    let semaphore = Arc::new(vthread::sync::Semaphore::with_wait_capacity(2, tasks + 8)?);
    let updates = Arc::new(vthread::sync::Mutex::with_wait_capacity(0, tasks + 8)?);
    let mut jobs = Vec::new();
    for id in 0..tasks {
        let semaphore = Arc::clone(&semaphore);
        let updates = Arc::clone(&updates);
        jobs.push(scope.spawn("soak-affine-worker", move || -> Result<()> {
            let owner = thread::current().id();
            let local = std::rc::Rc::new(id);
            for _ in 0..MUTEX_UPDATES_PER_TASK {
                let mut value = updates.lock()?;
                *value += 1;
                // Yield while owning the mutex to force queued ownership transfer.
                yield_now()?;
                assert_eq!(thread::current().id(), owner);
            }
            let _permit = semaphore.acquire()?;
            sleep(Duration::from_micros(100))?;
            assert_eq!(
                blocking::run(move || id.wrapping_mul(31))?,
                id.wrapping_mul(31)
            );
            assert_eq!(*local, id);
            assert_eq!(thread::current().id(), owner);
            Ok(())
        })?);
    }
    producer.join()??;
    consumer.join()??;
    let pair = exchange.finish()?;
    for mut job in jobs {
        job.join()??;
    }
    assert_eq!(*updates.try_lock()?, tasks * MUTEX_UPDATES_PER_TASK);
    assert_eq!(updates.waiting(), 0);
    Ok(pair)
}

pub(crate) fn cancel(runtime: &Runtime) -> Result<()> {
    runtime.run_scope(|scope| {
        let (parker, wake) = park_pair();
        let mut task = scope.spawn("soak-cancel", move || {
            parker.park_timeout(Duration::from_millis(1))
        })?;
        wake.cancel();
        wake.unpark();
        task.join()??;
        Ok(())
    })
}

#[cfg(test)]
#[path = "workload_test.rs"]
mod workload_test;
