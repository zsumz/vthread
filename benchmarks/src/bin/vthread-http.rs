use std::{env, net::SocketAddr, process::ExitCode};

#[path = "vthread_http/server.rs"]
mod server;

const DEFAULT_CONNECTIONS: usize = 4_096;
const DEFAULT_PORT: u16 = 8_080;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Config {
    address: SocketAddr,
    workers: usize,
    connections: usize,
}

impl Config {
    fn parse() -> Result<Self, String> {
        Self::parse_from(env::args().skip(1))
    }

    fn parse_from(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let workers = match args.next() {
            Some(value) => positive(&value, "workers")?,
            None => std::thread::available_parallelism()
                .map(usize::from)
                .map_err(|error| format!("detect workers: {error}"))?,
        };
        let port = match args.next() {
            Some(value) => value
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| "port must be an integer from 1 through 65535".to_owned())?,
            None => DEFAULT_PORT,
        };
        let connections = match args.next() {
            Some(value) => positive(&value, "connections")?,
            None => DEFAULT_CONNECTIONS,
        };
        if args.next().is_some() {
            return Err(usage());
        }
        Ok(Self {
            address: SocketAddr::from(([0, 0, 0, 0], port)),
            workers,
            connections,
        })
    }
}

fn main() -> ExitCode {
    match Config::parse().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(config: Config) -> Result<(), String> {
    let task_capacity = config
        .connections
        .checked_add(1)
        .ok_or_else(|| "connections is too large".to_owned())?;
    let runtime = vthread::Runtime::builder()
        .carriers(config.workers)
        .blocking_threads(1)
        .blocking_capacity(1)
        .io_capacity(task_capacity)
        .max_vthreads(task_capacity)
        .max_owned_scopes(1)
        .carrier_queue_capacity(task_capacity)
        .stack_size(64 * 1024)
        .stack_cache_capacity(config.connections.min(500))
        .build()
        .map_err(|error| error.to_string())?;
    let service = runtime
        .run_scope(|scope| server::run(scope, config.address))
        .map_err(|error| error.to_string());
    let shutdown = runtime.shutdown().map_err(|error| error.to_string());
    service.and(shutdown.map(|_| ()))
}

fn positive(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer"))
}

fn usage() -> String {
    "usage: vthread-http [workers [port [max-connections]]]".to_owned()
}

#[cfg(test)]
#[path = "vthread_http/main_test.rs"]
mod main_test;
