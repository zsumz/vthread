//! Controlled mutex handoffs, not an end-to-end runtime comparison.

#![forbid(unsafe_code)]

mod mutex_handoff;
mod mutex_handoff_linux;
mod mutex_handoff_tasks;

use mutex_handoff::{Config, Mode};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse(std::env::args().skip(1))?;
    let report = mutex_handoff::run(config)?;
    writeln!(
        std::io::stdout(),
        "schema=1 workload=mutex-mechanism mode={} iterations={} sender_tid={} recipient_tid={} sender_carrier={} recipient_carrier={} recipient_parks={} source_yields={} keeper_yields={} queued_handoffs={} sleep_observations={} elapsed_ns={} p50_ns={} p99_ns={} p999_ns={} max_ns={} latency=before-unlock-to-lock-return clock=instant-atomic control_cost=outside-latency-inside-process sleep_at_publication=not-claimed",
        config.mode.name(),
        config.iterations,
        report.sender_tid,
        report.recipient_tid,
        report.sender_carrier,
        report.recipient_carrier,
        report.recipient_parks,
        report.source_yields,
        report.keeper_yields,
        report.queued_handoffs,
        report.sleep_observations,
        report.elapsed.as_nanos(),
        report.percentile(50, 100),
        report.percentile(99, 100),
        report.percentile(999, 1000),
        report.maximum(),
    )?;
    // Raw samples are emitted after all tasks and carriers have drained. Their
    // formatting is included in process counters, never the acquisition window.
    let mut output = std::io::BufWriter::new(std::io::stdout().lock());
    for (index, nanoseconds) in report.samples.iter().enumerate() {
        writeln!(output, "sample={index} nanoseconds={nanoseconds}")?;
    }
    output.flush()?;
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mode = match args.next().as_deref() {
        Some("local") => Mode::Local,
        Some("remote-active") => Mode::RemoteActive,
        Some("remote-sleep-observed") => Mode::RemoteSleepObserved,
        _ => return Err("expected local|remote-active|remote-sleep-observed ITERATIONS".into()),
    };
    let iterations = args
        .next()
        .ok_or("missing iterations")?
        .parse::<usize>()
        .map_err(|_| "invalid iterations")?;
    if !(1..=100_000).contains(&iterations) || args.next().is_some() {
        return Err("expected 1..=100000 iterations and no extra arguments".into());
    }
    Ok(Config {
        mode,
        iterations,
        pin: true,
    })
}

#[cfg(test)]
#[path = "mutex_handoff_main_test.rs"]
mod mutex_handoff_main_test;
