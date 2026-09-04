//! Mutex handoffs with migration probes compiled out of measured closures.

macro_rules! spawn_mutex_tasks {
    ($scope:expr, $config:expr, $iterations:expr, $contended:expr, $observe:expr, $probes:expr) => {{
        let mutex = std::sync::Arc::new(may::sync::Mutex::new(0usize));
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..$config.tasks {
            let mutex = std::sync::Arc::clone(&mutex);
            let ready = std::sync::Arc::clone(&ready);
            let probe =
                $observe.then(|| std::sync::Arc::new($crate::may_placement::TaskProbe::new()));
            if let Some(probe) = &probe {
                $probes.push(std::sync::Arc::clone(probe));
            }
            may::go!($scope, move || {
                let mut trace = probe
                    .as_ref()
                    .map(|_| $crate::may_placement::TaskTrace::start());
                ready.fetch_add(1, std::sync::atomic::Ordering::Release);
                if $contended && $config.workers == 1 {
                    while ready.load(std::sync::atomic::Ordering::Acquire) != $config.tasks {
                        may::coroutine::yield_now();
                    }
                }
                for _ in 0..$iterations {
                    let mut value = mutex.lock().expect("mutex must not be poisoned");
                    trace
                        .iter_mut()
                        .for_each($crate::may_placement::TaskTrace::observe);
                    *value += 1;
                    if $contended {
                        if $config.workers == 1 {
                            may::coroutine::yield_now();
                        } else {
                            for _ in 0..32 {
                                std::hint::black_box(*value);
                            }
                        }
                    }
                }
                if let (Some(probe), Some(trace)) = (probe, trace) {
                    probe.record(trace);
                }
            });
        }
    }};
}

pub(crate) use spawn_mutex_tasks;

#[cfg(test)]
#[path = "may_mutex_test.rs"]
mod may_mutex_test;
