//! A dependency walk discovers transferable children and handles bounded saturation.

use crate::app;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use vthread::{Error, Result, Runtime, Spawner, error::CapacityResource};

#[derive(Default)]
struct Counts {
    visited: AtomicUsize,
    fallback: AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) checksum: u64,
    pub(crate) visited: usize,
    pub(crate) capacity_fallbacks: usize,
    pub(crate) cancelled: usize,
    pub(crate) application_failures: usize,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} nodes; checksum {}; {} capacity fallbacks; {} cancelled; {} application failures",
            self.visited,
            self.checksum,
            self.capacity_fallbacks,
            self.cancelled,
            self.application_failures
        )
    }
}

fn visit(spawner: &Spawner, counts: &Arc<Counts>, node: u64, depth: usize) -> Result<u64> {
    counts.visited.fetch_add(1, Ordering::SeqCst);
    if depth == 0 {
        return Ok(node);
    }
    let mut children = Vec::with_capacity(2);
    let mut total = node;
    for child in [node * 2, node * 2 + 1] {
        let next = spawner.clone();
        let observed = Arc::clone(counts);
        match spawner.spawn("discovered dependency", move || {
            visit(&next, &observed, child, depth - 1)
        }) {
            Ok(handle) => children.push(handle),
            Err(Error::Capacity {
                resource: CapacityResource::Tasks | CapacityResource::CarrierQueue,
                ..
            }) => {
                // Fixed recursion depth bounds the sequential fallback's native stack use.
                counts.fallback.fetch_add(1, Ordering::SeqCst);
                total += visit(spawner, counts, child, depth - 1)?;
            }
            Err(error) => return Err(error),
        }
    }
    for mut child in children {
        total += child.join()??;
    }
    Ok(total)
}

pub(crate) fn run() -> std::result::Result<Report, app::Failure> {
    app::run(
        Runtime::builder()
            .carriers(2)
            .max_vthreads(4)
            .max_owned_scopes(1)
            .stack_cache_capacity(0),
        |runtime| {
            runtime.run_scope(|scope| {
                let spawner = scope.spawner();
                scope
                    .spawn("walk owner", move || {
                        let mut speculative = spawner.spawn("speculative dependency", || {
                            vthread::sleep(Duration::from_secs(5))
                        })?;
                        speculative.cancel();
                        assert!(matches!(speculative.join()?, Err(Error::Cancelled)));
                        let mut failed = spawner
                            .spawn("invalid dependency", || Err::<(), _>("invalid dependency"))?;
                        assert_eq!(failed.join()?, Err("invalid dependency"));
                        let counts = Arc::new(Counts::default());
                        let checksum = visit(&spawner, &counts, 1, 3)?;
                        vthread::checkpoint()?;
                        Ok::<_, Error>(Report {
                            checksum,
                            visited: counts.visited.load(Ordering::SeqCst),
                            capacity_fallbacks: counts.fallback.load(Ordering::SeqCst),
                            cancelled: 1,
                            application_failures: 1,
                        })
                    })?
                    .join()?
            })
        },
    )
}

#[cfg(test)]
#[path = "discovered_work_test.rs"]
mod discovered_work_test;
