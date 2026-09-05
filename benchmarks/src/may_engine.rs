use crate::{
    config::{Config, Scenario},
    may_channel::spawn_channel_pairs,
    may_mutex::spawn_mutex_tasks,
    report::{Round, measure},
    wake_clock::{WakeClock, WakeStamp},
};
use std::{
    hint::black_box,
    io::{Read, Write},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

const STACK_SIZE: usize = 64 * 1024;

pub(crate) fn run(config: &Config) -> Result<(), String> {
    may::config()
        .set_workers(config.workers)
        .set_stack_size(STACK_SIZE / std::mem::size_of::<usize>())
        .set_pool_capacity(config.tasks);
    measure(config, |observe_placement| {
        run_round(config, observe_placement)
    })
}

fn run_yields(iterations: usize) {
    for index in 0..iterations {
        black_box(index);
        may::coroutine::yield_now();
    }
}

macro_rules! spawn_park_pairs {
    ($scope:expr, $tasks:expr, $iterations:expr, $observe:expr, $probes:expr) => {
        for _ in 0..$tasks / 2 {
            let a = Arc::new(OnceLock::<may::coroutine::Coroutine>::new());
            let b = Arc::new(OnceLock::<may::coroutine::Coroutine>::new());
            let probe = $observe.then(|| Arc::new(crate::may_placement::PairProbe::new()));
            if let Some(probe) = &probe {
                $probes.push(Arc::clone(probe));
            }
            let probe_a = probe.clone();
            let probe_b = probe;
            let own = Arc::clone(&a);
            let peer = Arc::clone(&b);
            may::go!($scope, move || {
                let mut trace = $observe.then(crate::may_placement::TaskTrace::start);
                assert!(own.set(may::coroutine::current()).is_ok());
                while peer.get().is_none() {
                    may::coroutine::yield_now();
                }
                if $observe {
                    trace.as_mut().expect("warm-up trace").observe();
                }
                for _ in 0..$iterations {
                    may::coroutine::park();
                    if $observe {
                        trace.as_mut().expect("warm-up trace").observe();
                    }
                    peer.get().expect("peer handle published").unpark();
                }
                if let (Some(probe), Some(trace)) = (probe_a, trace) {
                    probe.record(0, trace);
                }
            });
            may::go!($scope, move || {
                let mut trace = $observe.then(crate::may_placement::TaskTrace::start);
                assert!(b.set(may::coroutine::current()).is_ok());
                while a.get().is_none() {
                    may::coroutine::yield_now();
                }
                if $observe {
                    trace.as_mut().expect("warm-up trace").observe();
                }
                for _ in 0..$iterations {
                    a.get().expect("peer handle published").unpark();
                    may::coroutine::park();
                    if $observe {
                        trace.as_mut().expect("warm-up trace").observe();
                    }
                }
                if let (Some(probe), Some(trace)) = (probe_b, trace) {
                    probe.record(1, trace);
                }
            });
        }
    };
}

macro_rules! spawn_wake_tail_pairs {
    ($scope:expr, $tasks:expr, $iterations:expr) => {{
        let mut handles = Vec::with_capacity($tasks);
        for _ in 0..$tasks / 2 {
            let a = Arc::new(OnceLock::<may::coroutine::Coroutine>::new());
            let b = Arc::new(OnceLock::<may::coroutine::Coroutine>::new());
            let clock = WakeClock::new();
            let stamp_a = Arc::new(WakeStamp::new());
            let stamp_b = Arc::new(WakeStamp::new());
            let started = Arc::new(AtomicBool::new(false));
            let own = Arc::clone(&a);
            let peer = Arc::clone(&b);
            let own_stamp = Arc::clone(&stamp_a);
            let peer_stamp = Arc::clone(&stamp_b);
            let own_started = Arc::clone(&started);
            handles.push(may::go!($scope, move || {
                let mut samples = Vec::with_capacity($iterations);
                assert!(own.set(may::coroutine::current()).is_ok());
                own_started.store(true, Ordering::Release);
                while peer.get().is_none() {
                    may::coroutine::yield_now();
                }
                for _ in 0..$iterations {
                    may::coroutine::park();
                    samples.push(clock.elapsed(&own_stamp));
                    clock.publish(&peer_stamp);
                    peer.get().expect("peer handle published").unpark();
                }
                samples
            }));
            handles.push(may::go!($scope, move || {
                let mut samples = Vec::with_capacity($iterations);
                assert!(b.set(may::coroutine::current()).is_ok());
                while !started.load(Ordering::Acquire) || a.get().is_none() {
                    may::coroutine::yield_now();
                }
                for _ in 0..$iterations {
                    clock.publish(&stamp_a);
                    a.get().expect("peer handle published").unpark();
                    may::coroutine::park();
                    samples.push(clock.elapsed(&stamp_b));
                }
                samples
            }));
        }
        handles
    }};
}

fn run_round(config: &Config, observe_placement: bool) -> Result<Round, String> {
    let peer = match config.scenario {
        Scenario::Tcp { per_task } => {
            Some(crate::tcp_peer::EchoServer::start(config.tasks, per_task)?)
        }
        _ => None,
    };
    let address = peer.as_ref().map(crate::tcp_peer::EchoServer::address);
    let mut admission_ns = 0;
    let mut operation_latency_groups_ns = Vec::new();
    let mut placement_probes = Vec::new();
    let mut mutex_probes = Vec::new();
    may::coroutine::scope(|scope| {
        let started = Instant::now();
        match config.scenario {
            Scenario::Yield { per_task } => {
                for _ in 0..config.tasks {
                    may::go!(scope, move || run_yields(per_task));
                }
            }
            Scenario::Spawn => {
                for _ in 0..config.tasks {
                    may::go!(scope, || ());
                }
            }
            Scenario::Park { per_task } => {
                if observe_placement {
                    spawn_park_pairs!(scope, config.tasks, per_task, true, placement_probes);
                } else {
                    spawn_park_pairs!(scope, config.tasks, per_task, false, placement_probes);
                }
            }
            Scenario::Mutex {
                per_task,
                contended,
            } => {
                if observe_placement {
                    spawn_mutex_tasks!(scope, config, per_task, contended, true, mutex_probes);
                } else {
                    spawn_mutex_tasks!(scope, config, per_task, contended, false, mutex_probes);
                }
            }
            Scenario::Channel { per_task, capacity } => {
                macro_rules! spawn {
                    ($observe:expr) => {
                        match capacity {
                            None => spawn_channel_pairs!(
                                scope,
                                config.tasks,
                                per_task,
                                || may::sync::mpsc::channel(),
                                $observe,
                                placement_probes
                            ),
                            Some(capacity) => spawn_channel_pairs!(
                                scope,
                                config.tasks,
                                per_task,
                                || crate::may_bounded_channel::bounded(capacity),
                                $observe,
                                placement_probes
                            ),
                        }
                    };
                }
                if observe_placement {
                    spawn!(true);
                } else {
                    spawn!(false);
                }
            }
            Scenario::Tcp { per_task } => {
                let address = address.expect("TCP peer address");
                let mut clients = Vec::with_capacity(config.tasks);
                for _ in 0..config.tasks {
                    clients.push(may::go!(scope, move || {
                        run_tcp_round_trips(address, per_task)
                    }));
                }
                admission_ns = started.elapsed().as_nanos();
                for client in clients {
                    operation_latency_groups_ns.push(client.join()?);
                }
                return Ok::<(), String>(());
            }
            Scenario::WakeTail { per_task } => {
                let tasks = spawn_wake_tail_pairs!(scope, config.tasks, per_task);
                admission_ns = started.elapsed().as_nanos();
                for task in tasks {
                    operation_latency_groups_ns.push(task.join());
                }
                return Ok::<(), String>(());
            }
        }
        admission_ns = started.elapsed().as_nanos();
        Ok(())
    })?;
    if let Some(peer) = peer {
        peer.finish()?;
    }
    let (pair_owners, mut task_migrations) = crate::may_placement::summarize(&placement_probes);
    task_migrations.extend(mutex_probes.iter().map(|probe| probe.migrated()));
    Ok(Round {
        admission_ns,
        operation_latency_groups_ns,
        pair_owners,
        task_migrations,
        #[cfg(feature = "lifecycle-profiling")]
        lifecycle: None,
    })
}

fn run_tcp_round_trips(
    address: std::net::SocketAddr,
    iterations: usize,
) -> Result<Vec<u64>, String> {
    let mut stream = may::net::TcpStream::connect(address)
        .map_err(|error| format!("connect TCP benchmark client: {error}"))?;
    stream
        .set_nodelay(true)
        .map_err(|error| format!("configure TCP benchmark client: {error}"))?;
    let mut latencies = Vec::with_capacity(iterations);
    let mut byte = [0_u8; 1];
    for _ in 0..iterations {
        let started = Instant::now();
        stream
            .write_all(&byte)
            .map_err(|error| format!("write TCP benchmark byte: {error}"))?;
        stream
            .read_exact(&mut byte)
            .map_err(|error| format!("read TCP benchmark byte: {error}"))?;
        latencies.push(started.elapsed().as_nanos() as u64);
        black_box(byte);
    }
    Ok(latencies)
}
