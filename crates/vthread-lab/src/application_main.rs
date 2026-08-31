//! Public-API TCP service for reproducible application workloads.

#![forbid(unsafe_code)]

#[path = "application/failure.rs"]
mod failure;
#[path = "application/protocol.rs"]
mod protocol;
#[path = "application/server.rs"]
mod server;
#[path = "application/state.rs"]
mod state;

use std::io::{BufRead, Write};
use std::time::{Duration, Instant};
use vthread::{Result, Runtime, ScopeOptions};

fn main() -> Result<()> {
    let config = parse(std::env::args().skip(1)).map_err(std::io::Error::other)?;
    let runtime = Runtime::builder()
        .carriers(config.carriers)
        .max_vthreads(config.workers * 3 + 16)
        .carrier_queue_capacity(config.workers + 8)
        .stack_size(256 * 1024)
        .stack_cache_capacity(config.workers + 4)
        .io_capacity(config.workers + 8)
        .build()?;
    let owner = runtime.supervisor(ScopeOptions::default())?;
    let service = server::start(&owner, config.workers, config.queue, config.timeout)?;
    writeln!(
        std::io::stdout(),
        "{{\"event\":\"ready\",\"address\":\"{}\"}}",
        service.address
    )?;
    std::io::stdout().flush()?;
    // This control channel blocks only the ordinary OS owner, never a carrier.
    for line in std::io::stdin().lock().lines() {
        match line?.as_str() {
            "stats" => service.report(&runtime, "stats")?,
            "shutdown" => {
                let outcome = runtime.shutdown_until(Instant::now() + Duration::from_secs(5))?;
                assert!(matches!(
                    outcome,
                    vthread::lifecycle::ShutdownOutcome::Complete(_)
                ));
                service.report(&runtime, "stopped")?;
                break;
            }
            _ => return Err(std::io::Error::other("unknown control command").into()),
        }
        std::io::stdout().flush()?;
    }
    runtime.shutdown()?;
    service.join()?;
    owner.shutdown()?;
    Ok(())
}

struct Config {
    carriers: usize,
    workers: usize,
    queue: usize,
    timeout: Duration,
}

fn parse(args: impl Iterator<Item = String>) -> std::result::Result<Config, &'static str> {
    let values = args
        .map(|value| value.parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| "expected CARRIERS WORKERS QUEUE REQUEST_TIMEOUT_MS")?;
    if values.len() != 4
        || !(1..=16).contains(&values[0])
        || !(1..=256).contains(&values[1])
        || !(1..=256).contains(&values[2])
        || !(20..=60_000).contains(&values[3])
    {
        return Err("application limits exceeded");
    }
    Ok(Config {
        carriers: values[0],
        workers: values[1],
        queue: values[2],
        timeout: Duration::from_millis(values[3] as u64),
    })
}

#[cfg(test)]
#[path = "application_main_test.rs"]
mod application_main_test;
