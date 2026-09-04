use std::process::ExitCode;

#[cfg(feature = "allocation-probe")]
mod allocation_probe;
mod config;
#[cfg(feature = "lifecycle-profiling")]
mod lifecycle_profile;
mod may_bounded_channel;
mod may_engine;
mod may_placement;
mod report;
mod tcp_peer;
mod vthread_engine;
mod vthread_placement;
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
