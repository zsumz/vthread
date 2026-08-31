//! Reproducible runtime workloads, separate from the library API.

#![forbid(unsafe_code)]

mod network;
mod soak;
mod workload;

use std::{io::Write, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (duration, carriers, tasks) = parse_args(std::env::args().skip(1))?;
    let report = soak::run(duration, carriers, tasks)?;
    writeln!(
        std::io::stdout(),
        "{{\"schema\":1,\"workload\":\"mixed-soak\",\"connection_strategy\":\"persistent-pair\",\"carriers\":{carriers},\"tasks\":{tasks},\"iterations\":{},\"elapsed_ns\":{},\"spawned\":{},\"completed\":{},\"parks\":{},\"wakes\":{},\"stack_allocated\":{},\"stack_reused\":{}}}",
        report.iterations,
        report.elapsed.as_nanos(),
        report.stats.admitted(),
        report.stats.completed(),
        report.stats.parks(),
        report.stats.wakes(),
        report.stacks.allocated(),
        report.stacks.reused()
    )?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(Duration, usize, usize), Box<dyn std::error::Error>> {
    let mode = args.next().ok_or("expected: soak SECONDS CARRIERS TASKS")?;
    if mode != "soak" {
        return Err("unknown workload".into());
    }
    let seconds: u64 = args.next().ok_or("seconds")?.parse()?;
    let carriers: usize = args.next().ok_or("carriers")?.parse()?;
    let tasks: usize = args.next().ok_or("tasks")?.parse()?;
    if args.next().is_some()
        || seconds == 0
        || seconds > 86400
        || !(1..=64).contains(&carriers)
        || !(2..=4096).contains(&tasks)
    {
        return Err("invalid workload limits".into());
    }
    Ok((Duration::from_secs(seconds), carriers, tasks))
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
