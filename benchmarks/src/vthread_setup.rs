//! Runtime construction and opt-in Linux carrier pinning, entirely before warm-up.

use crate::config::Config;

pub(crate) fn build(config: &Config) -> Result<vthread::Runtime, String> {
    let cpus = config
        .pin_carriers
        .then(|| allowed_cpus(config.workers))
        .transpose()?;
    let runtime = vthread::Runtime::builder()
        .carriers(config.workers)
        .blocking_threads(1)
        .blocking_capacity(1)
        .io_capacity(config.tasks)
        .max_vthreads(config.vthread_capacity())
        .carrier_queue_capacity(config.tasks)
        .stack_size(64 * 1024)
        .stack_cache_capacity(config.tasks)
        .build()
        .map_err(|error| error.to_string())?;
    if let Some(cpus) = cpus {
        pin_carriers(&cpus)?;
    }
    Ok(runtime)
}

#[cfg(target_os = "linux")]
fn allowed_cpus(workers: usize) -> Result<Vec<usize>, String> {
    let status = std::fs::read_to_string("/proc/thread-self/status")
        .map_err(|error| format!("cannot inspect allowed CPUs: {error}"))?;
    first_cpus(cpu_list(&status)?, workers)
}

#[cfg(target_os = "linux")]
fn pin_carriers(cpus: &[usize]) -> Result<(), String> {
    use std::{
        fs,
        process::Command,
        time::{Duration, Instant},
    };

    let deadline = Instant::now() + Duration::from_secs(2);
    let tids = loop {
        let mut tids = Vec::new();
        for entry in fs::read_dir("/proc/self/task").map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let Ok(name) = fs::read_to_string(entry.path().join("comm")) else {
                continue;
            };
            // Linux comm truncates the longer Rust name to these 15 bytes.
            // TID order is only a pinning rank, not a claimed runtime CarrierId.
            if name.trim() == "vthread-carrier" {
                let tid = entry
                    .file_name()
                    .to_string_lossy()
                    .parse::<u32>()
                    .map_err(|error| format!("invalid carrier TID: {error}"))?;
                tids.push(tid);
            }
        }
        if tids.len() > cpus.len() {
            return Err("unexpected extra carrier threads; refusing partial pinning".into());
        }
        if tids.len() == cpus.len() {
            tids.sort_unstable();
            break tids;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "only {} of {} carriers became visible",
                tids.len(),
                cpus.len()
            ));
        }
        std::thread::yield_now();
    };
    for (rank, (tid, cpu)) in tids.into_iter().zip(cpus).enumerate() {
        let output = Command::new("taskset")
            .args(["--pid", "--cpu-list", &cpu.to_string(), &tid.to_string()])
            .output()
            .map_err(|error| format!("cannot run taskset for carrier {tid}: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "taskset failed for carrier {tid}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let status = fs::read_to_string(format!("/proc/self/task/{tid}/status"))
            .map_err(|error| format!("cannot verify pinned carrier {tid}: {error}"))?;
        if cpu_list(&status)? != cpu.to_string() {
            return Err(format!(
                "carrier {tid} did not retain its requested CPU {cpu}"
            ));
        }
        println!(
            "engine=vthread phase=worker-affinity rank={rank} os_tid={tid} cpu={cpu} verified=true"
        );
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn allowed_cpus(_: usize) -> Result<Vec<usize>, String> {
    Err("--pin-carriers requires Linux procfs and taskset".into())
}

#[cfg(not(target_os = "linux"))]
fn pin_carriers(_: &[usize]) -> Result<(), String> {
    Err("--pin-carriers requires Linux procfs and taskset".into())
}

#[cfg(any(target_os = "linux", test))]
fn cpu_list(status: &str) -> Result<&str, String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .map(str::trim)
        .filter(|list| !list.is_empty())
        .ok_or_else(|| "missing Cpus_allowed_list in thread status".into())
}

#[cfg(any(target_os = "linux", test))]
fn first_cpus(list: &str, workers: usize) -> Result<Vec<usize>, String> {
    if workers == 0 {
        return Err("pinning requires at least one carrier".into());
    }
    let mut cpus = Vec::new();
    let mut previous = None;
    for part in list.split(',') {
        let (start, end) = part.split_once('-').unwrap_or((part, part));
        let start = start.parse::<usize>().map_err(|_| "invalid CPU range")?;
        let end = end.parse::<usize>().map_err(|_| "invalid CPU range")?;
        if end < start || previous.is_some_and(|last| start <= last) {
            return Err("CPU ranges must be ordered and nonoverlapping".into());
        }
        previous = Some(end);
        cpus.extend((start..=end).take(workers - cpus.len()));
    }
    if cpus.len() != workers {
        return Err(format!(
            "requested {workers} carriers but only {} CPUs are allowed",
            cpus.len()
        ));
    }
    Ok(cpus)
}

#[cfg(test)]
#[path = "vthread_setup_test.rs"]
mod vthread_setup_test;
