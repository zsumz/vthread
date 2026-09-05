//! Optional per-endpoint API-call timing; the plain transfer loop is unchanged.

use crate::{channel_delivery::Delivery, vthread_channel::SharedRound};
use std::time::Instant;

pub(crate) fn run_shared(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
    capacity: usize,
    started: Instant,
) -> vthread::Result<SharedRound> {
    let lanes = tasks / 2;
    let (sender, receiver) = vthread::channel::bounded_with_wait_capacity(capacity, lanes)?;
    let mut receivers = Vec::with_capacity(lanes);
    let mut senders = Vec::with_capacity(lanes);
    // Match the plain control's admission order and normal placement.
    for producer in 0..lanes {
        let receiver = receiver.clone();
        receivers.push(scope.spawn("benchmark-shared-receiver", move || {
            let mut received = Vec::with_capacity(iterations);
            let mut latencies = Vec::with_capacity(iterations);
            for _ in 0..iterations {
                let before = Instant::now();
                let value = receiver.recv()?;
                let elapsed = before.elapsed().as_nanos() as u64;
                received.push(value);
                latencies.push(elapsed);
            }
            Ok::<_, vthread::Error>((received, latencies))
        })?);
        let sender = sender.clone();
        senders.push(scope.spawn("benchmark-shared-sender", move || {
            let mut latencies = Vec::with_capacity(iterations);
            for offset in 0..iterations {
                let value = producer * iterations + offset;
                let before = Instant::now();
                sender.send(value).map_err(|error| error.into_parts().0)?;
                latencies.push(before.elapsed().as_nanos() as u64);
            }
            Ok::<_, vthread::Error>(latencies)
        })?);
    }
    drop(sender);
    drop(receiver);
    let admission_ns = started.elapsed().as_nanos();
    let mut received = Vec::with_capacity(lanes);
    let mut latency_groups_ns = Vec::with_capacity(tasks);
    for receiver in &mut receivers {
        let (values, latencies) = receiver.join()??;
        received.push(values);
        latency_groups_ns.push(latencies);
    }
    for sender in &mut senders {
        latency_groups_ns.push(sender.join()??);
    }
    Ok(SharedRound {
        admission_ns,
        delivery: Delivery::new(received),
        latency_groups_ns,
    })
}

#[cfg(test)]
#[path = "vthread_channel_timed_test.rs"]
mod vthread_channel_timed_test;
