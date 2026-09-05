//! Paired historical controls and one shared bounded MPMC transfer workload.

use std::hint::black_box;

pub(crate) fn spawn_pairs(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
    capacity: usize,
) -> vthread::Result<()> {
    for _ in 0..tasks / 2 {
        let (to_b, from_a) = vthread::channel::bounded(capacity)?;
        let (to_a, from_b) = vthread::channel::bounded(capacity)?;
        drop(scope.spawn("benchmark-channel-a", move || {
            to_b.send(0).expect("peer must remain connected");
            for index in 0..iterations {
                let value = from_b.recv().expect("peer must send a value");
                black_box(value);
                if index + 1 != iterations {
                    to_b.send(value + 1).expect("peer must remain connected");
                }
            }
        })?);
        drop(scope.spawn("benchmark-channel-b", move || {
            for _ in 0..iterations {
                let value = from_a.recv().expect("peer must send a value");
                black_box(value);
                to_a.send(value + 1).expect("peer must remain connected");
            }
        })?);
    }
    Ok(())
}

pub(crate) fn spawn_shared(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
    capacity: usize,
) -> vthread::Result<Vec<vthread::JoinHandle<vthread::Result<Vec<usize>>>>> {
    let lanes = tasks / 2;
    let (sender, receiver) = vthread::channel::bounded_with_wait_capacity(capacity, lanes)?;
    let mut receivers = Vec::with_capacity(lanes);
    // Normal placement, alternating consumer/producer admission; no affinity hint
    // or diagnostic snapshot is inserted into the measured round.
    for producer in 0..lanes {
        let receiver = receiver.clone();
        receivers.push(scope.spawn("benchmark-shared-receiver", move || {
            let mut received = Vec::with_capacity(iterations);
            for _ in 0..iterations {
                received.push(receiver.recv()?);
            }
            Ok(received)
        })?);
        let sender = sender.clone();
        drop(scope.spawn("benchmark-shared-sender", move || {
            for offset in 0..iterations {
                // Configuration checks the complete identifier range for overflow.
                sender
                    .send(producer * iterations + offset)
                    .expect("shared channel receiver must remain connected");
            }
        })?);
    }
    drop(sender);
    drop(receiver);
    Ok(receivers)
}

#[cfg(test)]
#[path = "vthread_channel_test.rs"]
mod vthread_channel_test;
