//! Observed Linux thread identity/sleep; no claim about later scheduling state.

use std::io;
#[cfg(target_os = "linux")]
use std::{fs, io::Write};

pub(crate) fn error(message: impl Into<String>) -> vthread::Error {
    io::Error::other(message.into()).into()
}

#[cfg(target_os = "linux")]
pub(crate) fn tid() -> vthread::Result<usize> {
    fs::read_link("/proc/thread-self")?
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|tid| tid.parse().ok())
        .ok_or_else(|| error("invalid Linux thread identity"))
}

#[cfg(target_os = "linux")]
pub(crate) fn sleeping(tid: usize) -> vthread::Result<bool> {
    let stat = fs::read_to_string(format!("/proc/self/task/{tid}/stat"))?;
    if state(&stat)? != 'S' {
        return Ok(false);
    }
    // A second, non-atomic observation: no other workload, timer or I/O runs on
    // the recipient carrier. Spurious wakeups/preemption remain possible.
    let wchan = fs::read_to_string(format!("/proc/self/task/{tid}/wchan"))?;
    Ok(wchan.trim().contains("futex"))
}

#[cfg(target_os = "linux")]
pub(crate) fn pin(workers: usize) -> vthread::Result<()> {
    let status = fs::read_to_string("/proc/thread-self/status")?;
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .ok_or_else(|| error("missing allowed CPUs"))?;
    let cpus = cpus(list.trim(), workers)?;
    let mut tids = discover(workers)?;
    tids.sort_unstable();
    for (tid, cpu) in tids.into_iter().zip(cpus) {
        let output = std::process::Command::new("taskset")
            .args(["--pid", "--cpu-list", &cpu.to_string(), &tid.to_string()])
            .output()?;
        if !output.status.success() {
            return Err(error(String::from_utf8_lossy(&output.stderr)));
        }
        let status = fs::read_to_string(format!("/proc/self/task/{tid}/status"))?;
        if !status.lines().any(|line| {
            line.strip_prefix("Cpus_allowed_list:")
                .is_some_and(|list| list.trim() == cpu.to_string())
        }) {
            return Err(error("carrier pinning verification failed"));
        }
        writeln!(
            io::stdout(),
            "phase=affinity tid={tid} cpu={cpu} verified=true"
        )?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn discover(workers: usize) -> vthread::Result<Vec<usize>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let mut tids = Vec::new();
        for entry in fs::read_dir("/proc/self/task")? {
            let entry = entry?;
            if fs::read_to_string(entry.path().join("comm"))
                .unwrap_or_default()
                .trim()
                == "vthread-carrier"
            {
                tids.push(
                    entry
                        .file_name()
                        .to_string_lossy()
                        .parse::<usize>()
                        .map_err(|_| error("invalid carrier TID"))?,
                );
            }
        }
        if population_ready(tids.len(), workers)? {
            return Ok(tids);
        }
        if std::time::Instant::now() >= deadline {
            return Err(error(format!(
                "only {} of {workers} carrier names became visible",
                tids.len()
            )));
        }
        // Startup-only discovery, matching the existing benchmark's pinning
        // contract. No task has been admitted or acquisition timed yet.
        std::thread::yield_now();
    }
}

#[cfg(any(target_os = "linux", test))]
fn population_ready(observed: usize, expected: usize) -> vthread::Result<bool> {
    if observed > expected {
        return Err(error("unexpected extra carrier threads"));
    }
    Ok(observed == expected)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn tid() -> vthread::Result<usize> {
    Err(error("mutex mechanism fixture requires Linux procfs"))
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn sleeping(_: usize) -> vthread::Result<bool> {
    Err(error("sleep observation requires Linux procfs"))
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn pin(_: usize) -> vthread::Result<()> {
    Err(error("pinning requires Linux procfs and taskset"))
}

#[cfg(any(target_os = "linux", test))]
fn state(stat: &str) -> vthread::Result<char> {
    stat.rsplit_once(") ")
        .and_then(|(_, tail)| tail.chars().next())
        .filter(|state| state.is_ascii_uppercase() || *state == 't')
        .ok_or_else(|| error("invalid proc stat state"))
}

#[cfg(any(target_os = "linux", test))]
fn cpus(list: &str, count: usize) -> vthread::Result<Vec<usize>> {
    let mut result = Vec::new();
    let mut previous = None;
    for range in list.split(',') {
        let (first, last) = range.split_once('-').unwrap_or((range, range));
        let first: usize = first.parse().map_err(|_| error("invalid CPU list"))?;
        let last: usize = last.parse().map_err(|_| error("invalid CPU list"))?;
        if first > last || previous.is_some_and(|old| old >= first) {
            return Err(error("unordered CPU list"));
        }
        previous = Some(last);
        result.extend((first..=last).take(count.saturating_sub(result.len())));
    }
    if result.len() != count || count == 0 {
        return Err(error("insufficient allowed CPUs"));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "mutex_handoff_linux_test.rs"]
mod mutex_handoff_linux_test;
