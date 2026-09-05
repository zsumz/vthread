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

pub(crate) fn run(config: &Config) -> Result<(), String> {
    #[cfg(feature = "scheduler-profiling")]
    println!("engine=vthread phase=instrumentation scheduler_profiling=true headline=false");
    if let Scenario::ChannelMpmc { per_task, capacity } = config.scenario {
        println!(
            "engine=vthread phase=channel-contract channels=1 producers={} consumers={} messages_per_producer={} capacity={} wait_capacity_per_direction={} validation=exact-outside-elapsed timing=end-to-end receiver_recording=inside-elapsed topology=normal-placement latency_distribution={}",
            config.tasks / 2,
            config.tasks / 2,
            per_task,
            capacity,
            config.tasks / 2,
            config.sample_channel_latency,
        );
        if config.sample_channel_latency {
            crate::channel_latency::print_contract();
        }
    }
    let runtime = crate::vthread_setup::build(config)?;
    measure(config, |observe_placement| {
        run_round(&runtime, config, observe_placement)
    })?;
    runtime.shutdown().map_err(|error| error.to_string())?;
    #[cfg(feature = "scheduler-profiling")]
    crate::scheduler_profile::report(&mut std::io::stdout().lock(), &runtime.snapshot(), config)?;
    Ok(())
}

fn run_round(
    runtime: &vthread::Runtime,
    config: &Config,
    observe_placement: bool,
) -> Result<Round, String> {
    #[cfg(feature = "lifecycle-profiling")]
    let before = runtime.lifecycle_profile();
    let peer = match config.scenario {
        Scenario::Tcp { per_task } => {
            Some(crate::tcp_peer::EchoServer::start(config.tasks, per_task)?)
        }
        _ => None,
    };
    let address = peer.as_ref().map(crate::tcp_peer::EchoServer::address);
    let mut operation_latency_groups_ns = Vec::new();
    let mut pair_owners = Vec::new();
    let mut channel_delivery = None;
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
                Scenario::Park { per_task } => {
                    spawn_park_pairs(scope, config.tasks, per_task)?;
                    if observe_placement {
                        pair_owners = crate::vthread_placement::pair_owners(
                            &scope.runtime_snapshot(),
                            config.tasks,
                        );
                    }
                }
                Scenario::Mutex {
                    per_task,
                    contended,
                } => spawn_mutex_tasks(scope, config.tasks, per_task, config.workers, contended)?,
                Scenario::Channel { per_task, capacity } => {
                    crate::vthread_channel::spawn_pairs(
                        scope,
                        config.tasks,
                        per_task,
                        capacity.unwrap_or(1),
                    )?;
                    if observe_placement {
                        pair_owners = crate::vthread_placement::pair_owners(
                            &scope.runtime_snapshot(),
                            config.tasks,
                        );
                    }
                }
                Scenario::ChannelMpmc { per_task, capacity } => {
                    let shared = crate::vthread_channel::run_shared(
                        scope, config, per_task, capacity, started,
                    )?;
                    channel_delivery = Some(shared.delivery);
                    operation_latency_groups_ns = shared.latency_groups_ns;
                    return Ok(shared.admission_ns);
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
                        operation_latency_groups_ns.push(client.join()??);
                    }
                    return Ok(admission_ns);
                }
                Scenario::WakeTail { per_task } => {
                    let mut tasks = spawn_wake_tail_pairs(scope, config.tasks, per_task)?;
                    let admission_ns = started.elapsed().as_nanos();
                    if observe_placement {
                        pair_owners = crate::vthread_placement::pair_owners(
                            &scope.runtime_snapshot(),
                            config.tasks,
                        );
                    }
                    for task in &mut tasks {
                        operation_latency_groups_ns.push(task.join()??);
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
        operation_latency_groups_ns,
        pair_owners,
        task_migrations: Vec::new(),
        channel_delivery,
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
    contended: bool,
) -> vthread::Result<()> {
    let mutex = Arc::new(vthread::sync::Mutex::with_wait_capacity(0, tasks)?);
    let ready = Arc::new(AtomicUsize::new(0));
    for _ in 0..tasks {
        let mutex = Arc::clone(&mutex);
        let ready = Arc::clone(&ready);
        drop(scope.spawn("benchmark-mutex", move || {
            ready.fetch_add(1, Ordering::Release);
            if contended && workers == 1 {
                while ready.load(Ordering::Acquire) != tasks {
                    vthread::yield_now().expect("mutex barrier task must remain live");
                }
            }
            for _ in 0..iterations {
                let mut value = mutex.lock().expect("mutex must remain open");
                *value += 1;
                if contended {
                    if workers == 1 {
                        vthread::yield_now().expect("mutex owner must remain live");
                    } else {
                        for _ in 0..32 {
                            black_box(*value);
                        }
                    }
                }
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
