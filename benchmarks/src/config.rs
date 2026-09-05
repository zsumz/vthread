use std::{borrow::Cow, env};

#[derive(Clone, Copy)]
pub(crate) enum Engine {
    Vthread,
    May,
}

#[derive(Clone, Copy)]
pub(crate) enum Scenario {
    Yield {
        per_task: usize,
    },
    Spawn,
    Park {
        per_task: usize,
    },
    Mutex {
        per_task: usize,
        contended: bool,
    },
    Channel {
        per_task: usize,
        capacity: Option<usize>,
    },
    ChannelMpmc {
        per_task: usize,
        capacity: usize,
    },
    Tcp {
        per_task: usize,
    },
    WakeTail {
        per_task: usize,
    },
}

pub(crate) struct Config {
    pub(crate) engine: Engine,
    pub(crate) scenario: Scenario,
    pub(crate) workers: usize,
    pub(crate) tasks: usize,
    pub(crate) samples: usize,
    pub(crate) max_vthreads: Option<usize>,
    pub(crate) pin_carriers: bool,
    pub(crate) sample_channel_latency: bool,
}

impl Config {
    pub(crate) fn parse() -> Result<Self, String> {
        Self::parse_from(env::args().skip(1))
    }

    fn parse_from(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let engine = match args.next().as_deref() {
            Some("vthread") => Engine::Vthread,
            Some("may") => Engine::May,
            _ => return Err(usage()),
        };
        let scenario = match args.next().as_deref() {
            Some("yield") => Scenario::Yield {
                per_task: positive(&mut args, "yields-per-task")?,
            },
            Some("spawn") => Scenario::Spawn,
            Some("park") => Scenario::Park {
                per_task: positive(&mut args, "parks-per-task")?,
            },
            Some("mutex") => Scenario::Mutex {
                per_task: positive(&mut args, "locks-per-task")?,
                contended: true,
            },
            Some("mutex-uncontended") => Scenario::Mutex {
                per_task: positive(&mut args, "locks-per-task")?,
                contended: false,
            },
            Some("channel") => Scenario::Channel {
                per_task: positive(&mut args, "messages-per-task")?,
                capacity: None,
            },
            Some("channel-bounded-spsc") => Scenario::Channel {
                per_task: positive(&mut args, "messages-per-task")?,
                capacity: Some(channel_capacity(&mut args)?),
            },
            Some("channel-mpmc") if matches!(engine, Engine::Vthread) => Scenario::ChannelMpmc {
                per_task: positive(&mut args, "messages-per-task")?,
                capacity: channel_capacity(&mut args)?,
            },
            Some("channel-mpmc") => return Err("channel-mpmc is a vthread-only control".into()),
            Some("tcp") => Scenario::Tcp {
                per_task: positive(&mut args, "round-trips-per-task")?,
            },
            Some("wake-tail") => Scenario::WakeTail {
                per_task: positive(&mut args, "wakes-per-task")?,
            },
            _ => return Err(usage()),
        };
        let workers = positive(&mut args, "workers")?;
        let tasks = positive(&mut args, "tasks")?;
        let samples = positive(&mut args, "samples")?;
        if samples % 2 == 0 {
            return Err("samples must be odd so the median is one observation".into());
        }
        if matches!(
            scenario,
            Scenario::Park { .. } | Scenario::Channel { .. } | Scenario::WakeTail { .. }
        ) && (tasks < 2 || !tasks.is_multiple_of(2))
        {
            return Err("paired tasks must be an even number of at least two".into());
        }
        if matches!(
            scenario,
            Scenario::Mutex {
                contended: true,
                ..
            }
        ) && tasks < 2
        {
            return Err("mutex requires at least two contending tasks".into());
        }
        if matches!(
            scenario,
            Scenario::Mutex {
                contended: false,
                ..
            }
        ) && tasks != 1
        {
            return Err("uncontended mutex requires exactly one task".into());
        }
        if let Scenario::ChannelMpmc { per_task, .. } = scenario {
            if tasks < 4 || !tasks.is_multiple_of(2) {
                return Err(
                    "shared MPMC requires even tasks with at least two per direction".into(),
                );
            }
            if (tasks / 2).checked_mul(per_task).is_none()
                || per_task > isize::MAX as usize / std::mem::size_of::<usize>()
            {
                return Err("shared MPMC delivery evidence exceeds addressable storage".into());
            }
        }
        let mut max_vthreads = None;
        let mut pin_carriers = false;
        let mut sample_channel_latency = false;
        while let Some(option) = args.next() {
            if !matches!(engine, Engine::Vthread) {
                return Err("vthread-only option supplied to May".into());
            }
            match option.as_str() {
                "--max-vthreads" if max_vthreads.is_none() => {
                    let capacity = positive(&mut args, "max-vthreads")?;
                    if capacity < tasks.max(workers) {
                        return Err("max-vthreads must cover both tasks and workers".into());
                    }
                    max_vthreads = Some(capacity);
                }
                "--pin-carriers" if !pin_carriers => pin_carriers = true,
                "--sample-channel-latency"
                    if !sample_channel_latency
                        && matches!(scenario, Scenario::ChannelMpmc { .. }) =>
                {
                    sample_channel_latency = true;
                }
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            engine,
            scenario,
            workers,
            tasks,
            samples,
            max_vthreads,
            pin_carriers,
            sample_channel_latency,
        })
    }

    pub(crate) fn vthread_capacity(&self) -> usize {
        self.max_vthreads.unwrap_or(self.tasks.max(self.workers))
    }

    pub(crate) fn engine_name(&self) -> &'static str {
        match self.engine {
            Engine::Vthread => "vthread",
            Engine::May => "may",
        }
    }

    pub(crate) fn operation(&self) -> Cow<'static, str> {
        match self.scenario {
            Scenario::Yield { .. } => Cow::Borrowed("yield"),
            Scenario::Spawn => Cow::Borrowed("task"),
            Scenario::Park { .. } => Cow::Borrowed("park-handoff"),
            Scenario::Mutex {
                contended: true, ..
            } => Cow::Borrowed("mutex-handoff"),
            Scenario::Mutex {
                contended: false, ..
            } => Cow::Borrowed("mutex-uncontended"),
            Scenario::Channel { capacity: None, .. } => Cow::Borrowed("channel-handoff"),
            Scenario::Channel {
                capacity: Some(capacity),
                ..
            } => Cow::Owned(format!("bounded-spsc-channel-{capacity}-handoff")),
            Scenario::ChannelMpmc { capacity, .. } => {
                let suffix = if self.sample_channel_latency {
                    "-sampled"
                } else {
                    ""
                };
                Cow::Owned(format!("bounded-mpmc-channel-{capacity}-transfer{suffix}"))
            }
            Scenario::Tcp { .. } => Cow::Borrowed("tcp-round-trip"),
            Scenario::WakeTail { .. } => Cow::Borrowed("wake-to-resume"),
        }
    }

    pub(crate) fn operations(&self) -> u128 {
        let per_task = match self.scenario {
            Scenario::Yield { per_task }
            | Scenario::Park { per_task }
            | Scenario::Mutex { per_task, .. }
            | Scenario::Channel { per_task, .. }
            | Scenario::ChannelMpmc { per_task, .. }
            | Scenario::Tcp { per_task }
            | Scenario::WakeTail { per_task } => per_task,
            Scenario::Spawn => 1,
        };
        let producing_tasks = if matches!(self.scenario, Scenario::ChannelMpmc { .. }) {
            self.tasks / 2
        } else {
            self.tasks
        };
        (producing_tasks as u128) * (per_task as u128)
    }
}

fn positive(args: &mut impl Iterator<Item = String>, name: &str) -> Result<usize, String> {
    let value = args.next().ok_or_else(usage)?;
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if value == 0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(value)
}

fn channel_capacity(args: &mut impl Iterator<Item = String>) -> Result<usize, String> {
    let capacity = positive(args, "capacity")?;
    if capacity >= isize::MAX as usize {
        return Err("capacity must be less than isize::MAX".into());
    }
    Ok(capacity)
}

fn usage() -> String {
    format!(
        "{}\n       vthread-benchmarks vthread channel-mpmc <messages-per-producer> <capacity> <workers> <even-tasks>=4> <odd-samples> [--sample-channel-latency]",
        common_usage(),
    )
}

fn common_usage() -> String {
    "usage: vthread-benchmarks <vthread|may> yield <yields-per-task> <workers> <tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> spawn <workers> <tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> park <parks-per-task> <workers> <even-tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> mutex <locks-per-task> <workers> <tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> mutex-uncontended <locks-per-task> <workers> 1 <odd-samples>\n       vthread-benchmarks <vthread|may> channel <messages-per-task> <workers> <even-tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> channel-bounded-spsc <messages-per-task> <capacity> <workers> <even-tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> tcp <round-trips-per-task> <workers> <tasks> <odd-samples>\n       vthread-benchmarks <vthread|may> wake-tail <wakes-per-task> <workers> <even-tasks> <odd-samples>\n       vthread scenarios also accept: --max-vthreads <capacity> and --pin-carriers (Linux only)".into()
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

#[cfg(test)]
#[path = "channel_latency_config_test.rs"]
mod channel_latency_config_test;
