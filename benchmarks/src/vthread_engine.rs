use crate::{
    config::{Config, Scenario},
    report::{Round, measure},
    wake_clock::{WakeClock, WakeStamp},
};
use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

const STACK_SIZE: usize = 64 * 1024;

pub(crate) fn run(config: &Config) -> Result<(), String> {
    let runtime = vthread::Runtime::builder()
        .carriers(config.workers)
        .blocking_threads(1)
        .blocking_capacity(1)
        .io_capacity(config.tasks)
        .max_vthreads(config.tasks.max(config.workers))
        .carrier_queue_capacity(config.tasks)
        .stack_size(STACK_SIZE)
        .stack_cache_capacity(config.tasks)
        .build()
        .map_err(|error| error.to_string())?;
    measure(config, || run_round(&runtime, config))?;
    runtime
        .shutdown()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn run_round(runtime: &vthread::Runtime, config: &Config) -> Result<Round, String> {
    #[cfg(feature = "lifecycle-profiling")]
    let before = runtime.lifecycle_profile();
    let peer = match config.scenario {
        Scenario::Tcp { per_task } => {
            Some(crate::tcp_peer::EchoServer::start(config.tasks, per_task)?)
        }
        _ => None,
    };
    let address = peer.as_ref().map(crate::tcp_peer::EchoServer::address);
    let mut operation_latencies_ns = Vec::new();
    let admission_ns = runtime
        .run_scope(|scope| {
            let started = Instant::now();
            match config.scenario {
                Scenario::Yield { per_task } => {
                    for _ in 0..config.tasks {
                        drop(scope.spawn("benchmark-yield", move || run_yields(per_task))?);
                    }
                }
                Scenario::Spawn => {
                    for _ in 0..config.tasks {
                        drop(scope.spawn("benchmark-spawn", || ())?);
                    }
                }
                Scenario::Park { per_task } => spawn_park_pairs(scope, config.tasks, per_task)?,
                Scenario::Mutex { per_task } => {
                    spawn_mutex_tasks(scope, config.tasks, per_task, config.workers)?
                }
                Scenario::Channel { per_task } => {
                    spawn_channel_pairs(scope, config.tasks, per_task)?
                }
                Scenario::Tcp { per_task } => {
                    let address = address.expect("TCP peer address");
                    let mut clients = Vec::with_capacity(config.tasks);
                    for _ in 0..config.tasks {
                        clients.push(scope.spawn("benchmark-tcp", move || {
                            run_tcp_round_trips(address, per_task)
                        })?);
                    }
                    let admission_ns = started.elapsed().as_nanos();
                    for mut client in clients {
                        operation_latencies_ns.extend(client.join()??);
                    }
                    return Ok(admission_ns);
                }
                Scenario::WakeTail { per_task } => {
                    let mut tasks = spawn_wake_tail_pairs(scope, config.tasks, per_task)?;
                    let admission_ns = started.elapsed().as_nanos();
                    for task in &mut tasks {
                        operation_latencies_ns.extend(task.join()??);
                    }
                    return Ok(admission_ns);
                }
            }
            Ok(started.elapsed().as_nanos())
        })
        .map_err(|error| error.to_string())?;
    if let Some(peer) = peer {
        peer.finish()?;
    }
    #[cfg(feature = "lifecycle-profiling")]
    let lifecycle = Some(
        runtime
            .lifecycle_profile()
            .checked_delta(before)
            .ok_or_else(|| "lifecycle profile counters moved backward".to_owned())?,
    );
    Ok(Round {
        admission_ns,
        operation_latencies_ns,
        #[cfg(feature = "lifecycle-profiling")]
        lifecycle,
    })
}

fn run_tcp_round_trips(
    address: std::net::SocketAddr,
    iterations: usize,
) -> vthread::Result<Vec<u64>> {
    let stream = vthread::net::TcpStream::connect(address)?;
    stream.set_nodelay(true)?;
    let mut latencies = Vec::with_capacity(iterations);
    let mut byte = [0_u8; 1];
    for _ in 0..iterations {
        let started = Instant::now();
        stream.write_all(&byte)?;
        stream.read_exact(&mut byte)?;
        latencies.push(started.elapsed().as_nanos() as u64);
        black_box(byte);
    }
    Ok(latencies)
}

fn run_yields(iterations: usize) {
    for index in 0..iterations {
        black_box(index);
        vthread::yield_now().expect("benchmark task must remain live");
    }
}

fn spawn_park_pairs(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
) -> vthread::Result<()> {
    for _ in 0..tasks / 2 {
        let (park_a, wake_a) = vthread::parking::park_pair();
        let (park_b, wake_b) = vthread::parking::park_pair();
        drop(scope.spawn("benchmark-park-a", move || {
            for _ in 0..iterations {
                black_box(park_a.park().expect("park A must resume"));
                black_box(wake_b.unpark());
            }
        })?);
        drop(scope.spawn("benchmark-park-b", move || {
            for _ in 0..iterations {
                black_box(wake_a.unpark());
                black_box(park_b.park().expect("park B must resume"));
            }
        })?);
    }
    Ok(())
}

fn spawn_mutex_tasks(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
    workers: usize,
) -> vthread::Result<()> {
    let mutex = Arc::new(vthread::sync::Mutex::with_wait_capacity(0, tasks)?);
    let ready = Arc::new(AtomicUsize::new(0));
    for _ in 0..tasks {
        let mutex = Arc::clone(&mutex);
        let ready = Arc::clone(&ready);
        drop(scope.spawn("benchmark-mutex", move || {
            ready.fetch_add(1, Ordering::Release);
            if workers == 1 {
                while ready.load(Ordering::Acquire) != tasks {
                    vthread::yield_now().expect("mutex barrier task must remain live");
                }
            }
            for _ in 0..iterations {
                let mut value = mutex.lock().expect("mutex must remain open");
                *value += 1;
                if workers == 1 {
                    vthread::yield_now().expect("mutex owner must remain live");
                } else {
                    for _ in 0..32 {
                        black_box(*value);
                    }
                }
            }
        })?);
    }
    Ok(())
}

fn spawn_channel_pairs(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
) -> vthread::Result<()> {
    for _ in 0..tasks / 2 {
        let (to_b, from_a) = vthread::channel::bounded(1)?;
        let (to_a, from_b) = vthread::channel::bounded(1)?;
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

fn spawn_wake_tail_pairs(
    scope: &vthread::Scope<'_>,
    tasks: usize,
    iterations: usize,
) -> vthread::Result<Vec<vthread::JoinHandle<vthread::Result<Vec<u64>>>>> {
    let mut handles = Vec::with_capacity(tasks);
    for _ in 0..tasks / 2 {
        let (park_a, wake_a) = vthread::parking::park_pair();
        let (park_b, wake_b) = vthread::parking::park_pair();
        let clock = WakeClock::new();
        let stamp_a = Arc::new(WakeStamp::new());
        let stamp_b = Arc::new(WakeStamp::new());
        let started = Arc::new(AtomicBool::new(false));
        let own_stamp = Arc::clone(&stamp_a);
        let peer_stamp = Arc::clone(&stamp_b);
        let own_started = Arc::clone(&started);
        handles.push(scope.spawn("benchmark-wake-tail-a", move || {
            let mut samples = Vec::with_capacity(iterations);
            own_started.store(true, Ordering::Release);
            for _ in 0..iterations {
                park_a.park()?;
                samples.push(clock.elapsed(&own_stamp));
                clock.publish(&peer_stamp);
                wake_b.unpark();
            }
            Ok(samples)
        })?);
        handles.push(scope.spawn("benchmark-wake-tail-b", move || {
            let mut samples = Vec::with_capacity(iterations);
            while !started.load(Ordering::Acquire) {
                vthread::yield_now()?;
            }
            for _ in 0..iterations {
                clock.publish(&stamp_a);
                wake_a.unpark();
                park_b.park()?;
                samples.push(clock.elapsed(&stamp_b));
            }
            Ok(samples)
        })?);
    }
    Ok(handles)
}
