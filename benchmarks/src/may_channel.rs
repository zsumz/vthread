//! May channel workloads with warm-up-only execution-placement evidence.

macro_rules! spawn_channel_pairs {
    ($scope:expr, $tasks:expr, $iterations:expr, $channel:expr, $observe:expr, $probes:expr) => {
        for _ in 0..$tasks / 2 {
            let (to_b, from_a) = ($channel)();
            let (to_a, from_b) = ($channel)();
            let probe = $observe.then(|| {
                std::sync::Arc::new($crate::may_placement::PairProbe::new())
            });
            if let Some(probe) = &probe {
                $probes.push(std::sync::Arc::clone(probe));
            }
            let probe_a = probe.clone();
            let probe_b = probe;
            may::go!($scope, move || {
                let mut trace = probe_a
                    .as_ref()
                    .map(|_| $crate::may_placement::TaskTrace::start());
                to_b.send(0).expect("peer must remain connected");
                trace
                    .iter_mut()
                    .for_each($crate::may_placement::TaskTrace::observe);
                for index in 0..$iterations {
                    let value = from_b.recv().expect("peer must send a value");
                    trace
                        .iter_mut()
                        .for_each($crate::may_placement::TaskTrace::observe);
                    std::hint::black_box(value);
                    if index + 1 != $iterations {
                        to_b.send(value + 1).expect("peer must remain connected");
                        trace
                            .iter_mut()
                            .for_each($crate::may_placement::TaskTrace::observe);
                    }
                }
                if let (Some(probe), Some(trace)) = (probe_a, trace) {
                    probe.record(0, trace);
                }
            });
            may::go!($scope, move || {
                let mut trace = probe_b
                    .as_ref()
                    .map(|_| $crate::may_placement::TaskTrace::start());
                for _ in 0..$iterations {
                    let value = from_a.recv().expect("peer must send a value");
                    trace
                        .iter_mut()
                        .for_each($crate::may_placement::TaskTrace::observe);
                    std::hint::black_box(value);
                    to_a.send(value + 1).expect("peer must remain connected");
                    trace
                        .iter_mut()
                        .for_each($crate::may_placement::TaskTrace::observe);
                }
                if let (Some(probe), Some(trace)) = (probe_b, trace) {
                    probe.record(1, trace);
                }
            });
        }
    };
}

pub(crate) use spawn_channel_pairs;
