use crate::{Error, JoinHandle, Result, Runtime, Spawner, park_pair, support_test::until};
use std::{
    io::Write,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

const OWNERS: usize = 64;
const CHILDREN: usize = 128;

enum Event {
    Next(JoinHandle<Result<()>>),
    Fanout(Vec<JoinHandle<Result<()>>>),
}

fn handoff(index: usize, spawners: Arc<Vec<Spawner>>, sent: mpsc::Sender<Event>) -> Result<()> {
    if index < spawners.len() {
        let next_spawners = Arc::clone(&spawners);
        let next_sent = sent.clone();
        let child = spawners[index].spawn("owner handoff", move || {
            handoff(index + 1, next_spawners, next_sent)
        })?;
        sent.send(Event::Next(child)).unwrap();
        return Ok(());
    }
    let children = (0..CHILDREN)
        .map(|_| {
            spawners[index - 1].spawn("fanout child", || {
                let (wait, _wake) = park_pair();
                wait.park().map(|_| ())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    sent.send(Event::Fanout(children)).unwrap();
    Ok(())
}

#[test]
fn public_cross_owner_fanout_has_bounded_eviction_and_wakes_from_early_owner() {
    let runtime = Runtime::builder()
        .carriers(1)
        .max_vthreads(CHILDREN + 1)
        .stack_cache_capacity(1)
        .build()
        .unwrap();
    let supervisors = (0..OWNERS)
        .map(|_| runtime.supervisor().unwrap())
        .collect::<Vec<_>>();
    let spawners: Arc<Vec<Spawner>> =
        Arc::new(supervisors.iter().map(|owner| owner.spawner()).collect());
    runtime
        .run_scope(|scope| {
            let (sent, received) = mpsc::channel();
            let initial_spawners = Arc::clone(&spawners);
            let initial_sent = sent.clone();
            let mut current = spawners[0].spawn("owner handoff", move || {
                handoff(1, initial_spawners, initial_sent)
            })?;
            let mut children = loop {
                match received.recv_timeout(Duration::from_secs(10)).unwrap() {
                    Event::Next(next) => {
                        current.join()??;
                        drop(current);
                        current = next;
                    }
                    Event::Fanout(children) => break children,
                }
            };
            current.join()??;
            drop(current);
            until(|| runtime.snapshot().parked() == CHILDREN);
            let started = Instant::now();
            let mut eviction = scope.spawn("eviction probe", || ())?;
            eviction.join()?;
            let elapsed = started.elapsed();
            let (tokens, relays, links) = children[0].cancellation_token().graph_snapshot();
            assert!(relays <= 1, "{tokens} tokens, {relays} relays, {links} links");
            assert!(links <= 4 * (tokens + relays));
            assert!(elapsed < Duration::from_secs(2), "bridge eviction took {elapsed:?}");
            supervisors[0].cancel();
            for child in &mut children {
                assert!(matches!(child.join()?, Err(Error::Cancelled)));
            }
            writeln!(std::io::stdout().lock(),
                "cancellation-dense owners={OWNERS} children={CHILDREN} tokens={tokens} relays={relays} links={links} eviction_ns={}",
                elapsed.as_nanos()).unwrap();
            Ok(())
        })
        .unwrap();
    for supervisor in supervisors {
        supervisor.shutdown().unwrap();
    }
    assert_eq!(runtime.snapshot().active(), 0);
    runtime.shutdown().unwrap();
}
