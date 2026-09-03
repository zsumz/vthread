use vthread::diagnostics::LifecycleProfile;

pub(crate) struct Sample {
    reservation_ns: u64,
    envelope_ns: u64,
    inbox_ns: u64,
    stack_fiber_ns: u64,
    reclaim_ns: u64,
    completion_ns: u64,
    residual_ns: u128,
}

impl Sample {
    pub(crate) fn new(
        profile: LifecycleProfile,
        total_ns: u128,
        admission_ns: u128,
        tasks: usize,
    ) -> Result<Self, String> {
        let expected = u64::try_from(tasks).map_err(|_| "task count does not fit u64")?;
        let operations = [
            ("admission", profile.admission_operations()),
            ("stack/fiber", profile.stack_fiber_operations()),
            ("reclaim", profile.reclaim_operations()),
            ("completion", profile.completion_operations()),
        ];
        for (phase, actual) in operations {
            if actual != expected {
                return Err(format!(
                    "{phase} profile covered {actual} operations, expected {expected}"
                ));
            }
        }
        let stack_fiber_ns = profile.stack_fiber_nanoseconds();
        let reclaim_ns = profile.reclaim_nanoseconds();
        let completion_ns = profile.completion_nanoseconds();
        let attributed_ns = admission_ns
            .saturating_add(u128::from(stack_fiber_ns))
            .saturating_add(u128::from(reclaim_ns))
            .saturating_add(u128::from(completion_ns));
        Ok(Self {
            reservation_ns: profile.reservation_nanoseconds(),
            envelope_ns: profile.envelope_nanoseconds(),
            inbox_ns: profile.inbox_nanoseconds(),
            stack_fiber_ns,
            reclaim_ns,
            completion_ns,
            residual_ns: total_ns.saturating_sub(attributed_ns),
        })
    }
}

pub(crate) fn print_medians(config: &super::Config, samples: &[Sample]) {
    if samples.is_empty() {
        return;
    }
    let stack_fiber = median(samples, |sample| u128::from(sample.stack_fiber_ns));
    let reservation = median(samples, |sample| u128::from(sample.reservation_ns));
    let envelope = median(samples, |sample| u128::from(sample.envelope_ns));
    let inbox = median(samples, |sample| u128::from(sample.inbox_ns));
    let reclaim = median(samples, |sample| u128::from(sample.reclaim_ns));
    let completion = median(samples, |sample| u128::from(sample.completion_ns));
    let residual = median(samples, |sample| sample.residual_ns);
    println!(
        "engine={} phase=admission-detail workers={} tasks={} reservation_median_ns={} reservation_ns_per_task={:.2} envelope_median_ns={} envelope_ns_per_task={:.2} inbox_median_ns={} inbox_ns_per_task={:.2}",
        config.engine_name(),
        config.workers,
        config.tasks,
        reservation,
        per_task(reservation, config.tasks),
        envelope,
        per_task(envelope, config.tasks),
        inbox,
        per_task(inbox, config.tasks),
    );
    println!(
        "engine={} phase=lifecycle workers={} tasks={} stack_fiber_median_ns={} stack_fiber_ns_per_task={:.2} reclaim_median_ns={} reclaim_ns_per_task={:.2} completion_median_ns={} completion_ns_per_task={:.2} residual_median_ns={} residual_ns_per_task={:.2}",
        config.engine_name(),
        config.workers,
        config.tasks,
        stack_fiber,
        per_task(stack_fiber, config.tasks),
        reclaim,
        per_task(reclaim, config.tasks),
        completion,
        per_task(completion, config.tasks),
        residual,
        per_task(residual, config.tasks),
    );
}

fn median(samples: &[Sample], value: impl Fn(&Sample) -> u128) -> u128 {
    let mut values: Vec<_> = samples.iter().map(value).collect();
    values.sort_unstable();
    values[values.len() / 2]
}

fn per_task(nanoseconds: u128, tasks: usize) -> f64 {
    nanoseconds as f64 / tasks as f64
}

#[cfg(test)]
#[path = "lifecycle_profile_test.rs"]
mod lifecycle_profile_test;
