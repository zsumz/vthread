use std::process::ExitCode;

#[cfg(feature = "allocation-probe")]
mod allocation_probe;
mod channel_delivery;
mod channel_latency;
mod config;
#[cfg(feature = "handoff-profiling")]
mod handoff_profile;
#[cfg(feature = "lifecycle-profiling")]
mod lifecycle_profile;
mod report;
#[cfg(feature = "scheduler-profiling")]
mod scheduler_profile;
mod tcp_peer;
mod vthread_channel;
mod vthread_channel_timed;
mod vthread_engine;
mod vthread_placement;
mod vthread_setup;
mod wake_clock;

use config::Config;

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let config = Config::parse_from(args)?;
    println!(
        "engine=vthread phase=configuration max_vthreads={} workers={} tasks={} pin_carriers={}",
        config.vthread_capacity(),
        config.workers,
        config.tasks,
        config.pin_carriers,
    );
    vthread_engine::run(&config)
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
