use std::{env, hint::black_box, process::ExitCode, time::Instant};

#[cfg(feature = "allocation-probe")]
mod allocation_probe;

const STACK_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum Engine {
    Vthread,
    May,
}

#[derive(Clone, Copy)]
enum Scenario {
    Yield { per_task: usize },
    Spawn,
}

struct Config {
    engine: Engine,
    scenario: Scenario,
    workers: usize,
    tasks: usize,
    samples: usize,
}

impl Config {
    fn parse() -> Result<Self, String> {
        let mut args = env::args().skip(1);
        let engine = match args.next().as_deref() {
            Some("vthread") => Engine::Vthread,
            Some("may") => Engine::May,
            _ => return Err(usage()),
        };
        let scenario = match args.next().as_deref() {
            Some("yield") => Scenario::Yield {
                per_task: positive(&mut args, "yields-per-task")?,
            },
            Some("spawn") => Scenario::Spawn,
            _ => return Err(usage()),
        };
        let workers = positive(&mut args, "workers")?;
        let tasks = positive(&mut args, "tasks")?;
        let samples = positive(&mut args, "samples")?;
        if samples % 2 == 0 {
            return Err("samples must be odd so the median is one observation".into());
        }
        if args.next().is_some() {
            return Err(usage());
        }
        Ok(Self {
            engine,
            scenario,
            workers,
            tasks,
            samples,
        })
    }

    fn engine_name(&self) -> &'static str {
        match self.engine {
            Engine::Vthread => "vthread",
            Engine::May => "may",
        }
    }

    fn operation(&self) -> &'static str {
        match self.scenario {
            Scenario::Yield { .. } => "yield",
            Scenario::Spawn => "task",
        }
    }

    fn operations(&self) -> u128 {
        match self.scenario {
            Scenario::Yield { per_task } => (self.tasks as u128) * (per_task as u128),
            Scenario::Spawn => self.tasks as u128,
        }
    }
}

fn main() -> ExitCode {
    let result = Config::parse().and_then(|config| run(&config));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(config: &Config) -> Result<(), String> {
    match config.engine {
        Engine::Vthread => run_vthread(config),
        Engine::May => run_may(config),
    }
}

fn run_vthread(config: &Config) -> Result<(), String> {
    let runtime = vthread::Runtime::builder()
        .carriers(config.workers)
        .blocking_threads(1)
        .blocking_capacity(1)
        .max_vthreads(config.tasks.max(config.workers))
        .carrier_queue_capacity(config.tasks)
        .stack_size(STACK_SIZE)
        .stack_cache_capacity(config.tasks)
        .build()
        .map_err(|error| error.to_string())?;
    measure(config, || {
        runtime
            .run_scope(|scope| {
                let started = Instant::now();
                for _ in 0..config.tasks {
                    let scenario = config.scenario;
                    drop(scope.spawn("benchmark", move || run_vthread_task(scenario))?);
                }
                Ok(started.elapsed().as_nanos())
            })
            .map_err(|error| error.to_string())
    })?;
    runtime
        .shutdown()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn run_vthread_task(scenario: Scenario) {
    if let Scenario::Yield { per_task } = scenario {
        for index in 0..per_task {
            black_box(index);
            vthread::yield_now().expect("benchmark task must remain live");
        }
    }
}

fn run_may(config: &Config) -> Result<(), String> {
    may::config()
        .set_workers(config.workers)
        // May configures stacks in machine words; vthread configures bytes.
        .set_stack_size(STACK_SIZE / std::mem::size_of::<usize>())
        .set_pool_capacity(config.tasks);
    measure(config, || {
        let mut admission_ns = 0;
        may::coroutine::scope(|scope| {
            let started = Instant::now();
            for _ in 0..config.tasks {
                let scenario = config.scenario;
                may::go!(scope, move || run_may_task(scenario));
            }
            admission_ns = started.elapsed().as_nanos();
        });
        Ok(admission_ns)
    })
}

fn run_may_task(scenario: Scenario) {
    if let Scenario::Yield { per_task } = scenario {
        for index in 0..per_task {
            black_box(index);
            may::coroutine::yield_now();
        }
    }
}

fn measure(config: &Config, mut round: impl FnMut() -> Result<u128, String>) -> Result<(), String> {
    round()?;
    let mut samples = Vec::with_capacity(config.samples);
    let mut admission_samples = Vec::with_capacity(config.samples);
    let mut drain_samples = Vec::with_capacity(config.samples);
    #[cfg(feature = "allocation-probe")]
    let mut allocation_samples = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        #[cfg(feature = "allocation-probe")]
        allocation_probe::begin();
        let started = Instant::now();
        let admission = round()?;
        let total = started.elapsed().as_nanos();
        #[cfg(feature = "allocation-probe")]
        allocation_samples.push(allocation_probe::finish());
        samples.push(total);
        admission_samples.push(admission);
        drain_samples.push(total.saturating_sub(admission));
    }
    samples.sort_unstable();
    admission_samples.sort_unstable();
    drain_samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let per_operation = median as f64 / config.operations() as f64;
    println!(
        "engine={} operation={} workers={} tasks={} median_ns={} ns_per_operation={:.2} samples={:?}",
        config.engine_name(),
        config.operation(),
        config.workers,
        config.tasks,
        median,
        per_operation,
        samples
    );
    if matches!(config.scenario, Scenario::Spawn) {
        println!(
            "engine={} phase=spawn workers={} tasks={} admission_median_ns={} drain_median_ns={} admission_samples={:?} drain_samples={:?}",
            config.engine_name(),
            config.workers,
            config.tasks,
            admission_samples[admission_samples.len() / 2],
            drain_samples[drain_samples.len() / 2],
            admission_samples,
            drain_samples,
        );
    }
    #[cfg(feature = "allocation-probe")]
    print_allocation_medians(config, &mut allocation_samples);
    Ok(())
}

#[cfg(feature = "allocation-probe")]
fn print_allocation_medians(config: &Config, samples: &mut [allocation_probe::Counts]) {
    let middle = samples.len() / 2;
    samples.sort_unstable_by_key(|sample| sample.allocations);
    let allocations = samples[middle].allocations;
    samples.sort_unstable_by_key(|sample| sample.deallocations);
    let deallocations = samples[middle].deallocations;
    samples.sort_unstable_by_key(|sample| sample.allocated_bytes);
    let allocated_bytes = samples[middle].allocated_bytes;
    println!(
        "engine={} phase=allocation workers={} tasks={} allocations={} deallocations={} allocated_bytes={} allocations_per_task={:.2} bytes_per_task={:.2}",
        config.engine_name(),
        config.workers,
        config.tasks,
        allocations,
        deallocations,
        allocated_bytes,
        allocations as f64 / config.tasks as f64,
        allocated_bytes as f64 / config.tasks as f64,
    );
}

fn positive(args: &mut impl Iterator<Item = String>, name: &str) -> Result<usize, String> {
    let value = args.next().ok_or_else(usage)?;
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if value == 0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(value)
}

fn usage() -> String {
    "usage: vthread-benchmarks <vthread|may> yield <yields-per-task> <workers> <tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> spawn <workers> <tasks> <odd-samples>".into()
}
