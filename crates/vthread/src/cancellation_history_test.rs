use super::CancellationToken;
use crate::{
    Error, JoinHandle, Result, Runtime, Spawner,
    parking::{Parker, Unparker, park_pair},
};
use std::{
    io::Write,
    sync::mpsc,
    time::{Duration, Instant},
};

const GENERATIONS: usize = 100_000;

#[test]
fn dropped_intermediate_tokens_do_not_retain_history() {
    let ancestor = CancellationToken::root(2);
    let mut current = ancestor.child_token();
    for generation in 0..GENERATIONS {
        current = current.child_token();
        if generation % 64 == 0 {
            let (nodes, relays, edges) = current.graph_snapshot();
            assert!(
                nodes <= 2 && relays == 0 && edges <= 1,
                "generation {generation}: {nodes} nodes, {relays} relays, {edges} edges"
            );
        }
    }
    ancestor.cancel();
    assert!(current.is_cancelled());
    drop(current);
    assert_eq!(ancestor.graph_snapshot(), (1, 0, 0));
}

type Handoff = (JoinHandle<Result<()>>, Unparker);

fn generation(spawner: Spawner, sent: mpsc::SyncSender<Handoff>, gate: Parker) -> Result<()> {
    gate.park()?;
    let (gate, wake) = park_pair();
    let next_spawner = spawner.clone();
    let next_sent = sent.clone();
    let child = spawner.spawn("generation", move || {
        generation(next_spawner, next_sent, gate)
    })?;
    sent.send((child, wake)).unwrap();
    Ok(())
}

fn latency(token: &CancellationToken) -> (u128, u128) {
    let start = Instant::now();
    for _ in 0..10_000 {
        assert!(!std::hint::black_box(token).is_cancelled());
    }
    let checks = start.elapsed().as_nanos();
    let start = Instant::now();
    for _ in 0..100 {
        let probe = token.child_token();
        probe.cancel();
        assert!(probe.is_cancelled());
    }
    (checks, start.elapsed().as_nanos())
}

#[test]
fn sequential_dynamic_generations_keep_cancellation_live_and_bounded() {
    for cancel_owner in [false, true] {
        let runtime = Runtime::builder()
            .carriers(2)
            .max_vthreads(2)
            .stack_cache_capacity(2)
            .build()
            .unwrap();
        runtime.run_scope(|scope| {
            let spawner = scope.spawner();
            let (sent, received) = mpsc::sync_channel(1);
            let (gate, mut wake) = park_pair();
            let mut current = scope.spawn("first", move || generation(spawner, sent, gate))?;
            let ancestor = current.cancellation_token();
            let beginning = latency(&ancestor);
            let mut peak = (0, 0);
            for index in 0..GENERATIONS {
                wake.unpark();
                current.join()??;
                drop(current);
                (current, wake) = received.recv_timeout(Duration::from_secs(10)).unwrap();
                let snapshot = current.cancellation_token().graph_snapshot();
                peak = (peak.0.max(snapshot.0), peak.1.max(snapshot.2));
                assert!(snapshot.0 <= 8 && snapshot.1 <= 1 && snapshot.2 <= 12,
                    "generation {index}: {snapshot:?}");
                assert!(runtime.snapshot().tasks().len() <= 2);
            }
            let token = current.cancellation_token();
            let ending = latency(&token);
            // Timing is diagnostic with a generous noise allowance; state bounds above
            // are the deterministic history-regression gate, not a microbenchmark SLA.
            assert!(ending.0 <= beginning.0 * 128 + 10_000_000);
            assert!(ending.1 <= beginning.1 * 128 + 10_000_000);
            crate::support_test::until(|| runtime.snapshot().parked() == 1);
            let started = Instant::now();
            if cancel_owner { scope.cancel(); } else { ancestor.cancel(); }
            let cancel_ns = started.elapsed().as_nanos();
            assert!(matches!(current.join()?, Err(Error::Cancelled)));
            assert!(token.is_cancelled());
            writeln!(std::io::stdout().lock(), "cancellation-history generations={GENERATIONS} owner={cancel_owner} peak={peak:?} start_ns={beginning:?} end_ns={ending:?} cancel_ns={cancel_ns}").unwrap();
            Ok(())
        }).unwrap();
        runtime.shutdown().unwrap();
    }
}
