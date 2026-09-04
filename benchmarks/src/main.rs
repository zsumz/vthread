use std::process::ExitCode;

#[cfg(feature = "allocation-probe")]
mod allocation_probe;
mod config;
#[cfg(feature = "lifecycle-profiling")]
mod lifecycle_profile;
mod may_engine;
mod report;
mod tcp_peer;
mod vthread_engine;
mod wake_clock;

use config::{Config, Engine};

fn main() -> ExitCode {
    let result = Config::parse().and_then(|config| match config.engine {
        Engine::Vthread => vthread_engine::run(&config),
        Engine::May => may_engine::run(&config),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
