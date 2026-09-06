# Scheduler benchmarks

See the [current performance status](performance-status.md) for landed work,
rejected experiments, remaining gaps and next steps. The
[2026-09-05 checkpoint overview](checkpoint-overview.md) is the last complete May
comparison, not a current-HEAD benchmark run.

This standalone harness compares the default `vthread` scheduler with May 0.3.51. It uses
structured scopes, 64 KiB stacks, the same worker and task counts, one untimed warm-up, and an odd
number of measured rounds. Runtime construction is outside the measurements; scope, task,
operation, completion, and join work is inside them.

Vthread defaults its runtime admission capacity to the task count (or worker count,
if larger). Append `--max-vthreads <capacity>` to any vthread scenario to measure
spare provisioned capacity without changing the live workload. The configuration
line records the effective limit. This option is rejected for May, which does not
provide an equivalent admission bound. For example, compare 64 live tasks in tightly
sized and default-sized vthread runtimes:

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread park 10000 4 64 9
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread park 10000 4 64 9 --max-vthreads 65536
```

See [the capacity review](capacity-review.md) for measured spare-capacity effects
and the runtime experiments rejected by lifecycle regression checks.

On Linux, append `--pin-carriers` to explicitly pin each vthread carrier to a distinct
CPU from the process's allowed mask. This requires procfs and `taskset`, runs before
warm-up, verifies each thread's resulting mask, and fails if too few CPUs are allowed.
Only this benchmark's carrier threads are pinned; task admission and affinity policies
are unchanged. The printed rank orders OS thread IDs, not runtime carrier IDs.
Without the flag, no per-carrier affinity is changed. It can be combined with
`--max-vthreads` in either order and is rejected for May, whose defaults are unchanged.

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread mutex 100000 4 64 9 --pin-carriers
```

Keep pinned and default results separately labeled. Pinning does not make May's
migrating coroutines equivalent to vthread's non-migrating tasks. The
[mutex placement review](mutex-placement-review.md) records the controlled comparison.

Build once, then run each engine in a fresh process. Pin single-worker measurements to one CPU
when `taskset` is available:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread yield 100000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may yield 100000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread spawn 1 1000 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may spawn 1 1000 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread park 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may park 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread mutex 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may mutex 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread mutex-uncontended 1000000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may mutex-uncontended 1000000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread channel 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may channel 100000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread channel-bounded-spsc 100000 1 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may channel-bounded-spsc 100000 1 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread wake-tail 10000 1 2 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may wake-tail 10000 1 2 11
```

Exercise sustained four-carrier handoffs on four pinned CPUs with enough tasks to keep every
carrier active:

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread park 20000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may park 20000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread mutex 2000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may mutex 2000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel 20000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may channel 20000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel-bounded-spsc 20000 1 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may channel-bounded-spsc 20000 1 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread wake-tail 10000 4 64 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may wake-tail 10000 4 64 11
```

The scenarios have deliberately narrow operation contracts:

| Scenario | Reported operation |
| --- | --- |
| `yield` | One cooperative yield |
| `spawn` | One admitted, completed, reclaimed, and drained task |
| `park` | One half of a paired park/unpark handoff |
| `mutex` | One contended lock acquisition and release |
| `mutex-uncontended` | One immediately available lock acquisition and release |
| `channel` | One message handoff in a paired bounded-channel exchange |
| `channel-bounded-spsc` | One message handoff through a paired capacity-gated SPSC channel |
| `channel-mpmc` | One value transferred through one shared bounded MPMC channel; vthread-only control |
| `tcp` | One write/read echo round trip on a task-owned connection |
| `wake-tail` | One timestamped wake-to-resume handoff |

`park`, the paired channel scenarios, and `wake-tail` require an even task count of at least two. The
historical `channel` case retains vthread's capacity-one channel against May's unbounded MPSC
channel. `channel-bounded-spsc` gives both engines the requested positive capacity; May 0.3.51 has
no bounded channel, so its SPSC channel is capacity-gated by May's coroutine-aware semaphore. With one worker, the
mutex benchmark yields while holding the lock to force FIFO handoffs. With multiple workers, it
performs the same 32 black-box operations in each engine's critical section so native contention is
exercised without an all-task startup deadlock.

`channel-mpmc` is a separate **vthread-only baseline/candidate control**, not a new
May comparison. It requires an even task count of at least four, split equally into
producers and consumers. They share one channel and are admitted as alternating
consumer/producer tasks under normal placement. Each producer sends the requested
number of distinct `usize` identifiers; each consumer receives that many values.
The operation denominator is `tasks / 2 * messages-per-producer`, counting each
transferred value once, not both endpoint calls. For example:

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 20000 1 4 64 9 --pin-carriers
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 20000 64 4 64 9 --pin-carriers
```

The channel's waiter bound is exactly the number of tasks per direction. There is
no co-location hint, startup barrier, additional retry loop or shared measurement
lock. Receiver-local vectors record each value during the measured interval. Every
warm-up and measured round must pass exact count, identifier-range and uniqueness
checks, performed **after elapsed timing and allocation recording stop**. Vector
allocation and recording remain part of end-to-end timing; validation and freeing
those vectors are outside it. Whole-process `perf stat` includes validation as well
as warm-up and shutdown, so its cycles are not pure channel cycles.

Add `--sample-channel-latency` to collect one `Instant` interval around every send
and receive API call. This is a separate `-sampled` operation label: the throughput
denominator still counts transferred values once, while each transfer contributes
two endpoint-call observations. Directional reports keep sender and receiver tails
separate (p50/p90/p95/p99/p99.9/p99.99/max), with per-stream median and p99.9 spread.
Every round, including warm-up, must supply exactly one sample per call per task.
Streams are logical admission slots across rounds, consumers first then producers;
they are not one persistent task identity. Warm-up observations are excluded.

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 20000 1 4 64 9 --pin-carriers --sample-channel-latency
```

The sampled path retains sender join handles and preallocates task-local timing
vectors inside the elapsed interval. Clock reads and recording perturb scheduling;
clock overhead is not subtracted. Summarization and validation are outside elapsed
timing, but included in whole-process hardware counters. Compare sampled baseline
and candidate separately, and keep the untimed control for throughput acceptance.
This closed-loop, fixed-quota workload measures API-call latency and per-stream
spread, **not** message arrival-to-delivery latency, offered-load backpressure,
maximum waiter age or a starvation/fairness bound. No extra shared measurement lock,
affinity hint or per-operation branch is added to the plain transfer loop.

The round report includes median, p95, p99, maximum, and every whole-round sample. `tcp` and
`wake-tail` additionally retain per-task latency streams across measured rounds and print
their median, p95, p99, p99.9, p99.99, and maximum. Wake timestamps move through a cache-aligned
atomic slot so the observer does not add a native mutex to every sample. The TCP case uses a native
loopback echo peer for both engines;
its per-operation distribution is more useful than its whole-round total, which also includes peer
startup and shutdown. Use a quiet machine and compare distributions as well as medians. Carrier
placement, frequency scaling, and host scheduling make short and multi-worker samples noisy.
May can move coroutines through work stealing and wake routing; vthread deliberately keeps every
started task on its owner carrier. The harness therefore compares the complete runtime contracts,
not identical scheduling policies. Per-task and paired-tail summaries expose fairness. For paired
park and channel workloads, a warm-up-only observer reports May's final execution-worker pairs and
per-task migration alongside vthread's immutable owner-carrier pairs. No placement observer runs in
the measured rounds. The mutex warm-up also reports May task migration after acquisitions.
May 0.3.51 also pins its individual OS workers by default; vthread leaves its carriers
unpinned within the process CPU mask. A shared `taskset` mask does not make those worker
placement policies identical. See the [dequeue review](dequeue-review.md) for the profile,
code-generation experiment, tail checks, and retention decision.
These observations describe the instrumented warm-up; they are not a count of migrations during
the measured rounds and final pair placement does not describe every handoff. Vthread's wake-tail
warm-up also reports immutable owner-carrier pairs.

For a readiness comparison, local socket creation must be permitted:

```sh
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread tcp 1000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may tcp 1000 1 1 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread tcp 500 4 8 11
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks may tcp 500 4 8 11
```

For attributed vthread lifecycle timings, rebuild with the opt-in profiling feature:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --features lifecycle-profiling
benchmarks/target/release/vthread-benchmarks vthread spawn 1 1000 101
benchmarks/target/release/vthread-benchmarks vthread spawn 1 10000 101
```

The additional lifecycle lines split producer admission into reservation, typed envelope, and inbox
transfer, then separate stack/fiber materialization, physical reclaim, terminal completion
publication, post-drain diagnostic-record retirement, and unattributed scheduler work. The profiler
verifies that every requested task appears exactly once in admission, all three carrier phases, and
scope retirement. Producer and carrier phases overlap, so they are not additive and the saturating
residual is only a lower bound. Clock reads and atomic accounting intentionally make profiled totals
slower; use the default build above for engine-to-engine comparisons.

For admission/idle attribution without per-task clocks or shared counter atomics:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --features scheduler-profiling
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread spawn 4 10000 501
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread park 10000 4 64 11 --max-vthreads 65536
```

Carrier-local counters are copied through existing snapshot publication; the harness
reads them **only after shutdown**, never between rounds. Final output checks all
warm-up and measured task counts and reports receive-size histograms, full-window
deferrals, dispatches between idle entries, poll probes/hits and wait calls. Totals
include startup, warm-up and shutdown, not just measured intervals. Wait calls do not
prove kernel sleeps; returns without task work can reflect timers or control events.
This opt-in feature still changes code/layout and snapshot-copy work. Its timings are
diagnostic, not headline comparisons; disable all profiling for performance acceptance.

For channel progress and handoff/idle stage attribution, separately from headline runs:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --features handoff-profiling
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 2000 1 4 64 3 --pin-carriers
```

This feature implies `scheduler-profiling` and adds owner-local duration histograms
and channel transfer/notification/park/retry counters. The final-only report checks
their conservation and separates visible-work polls, control-only polls, exhausted
polls, wait API calls and condition-variable invocations. Native callers without a
carrier route are omitted. Durations overlap and include clock/preemption costs;
they must not be added as CPU time. The clocks can significantly amplify channel
lock contention. Use these counts to form experiments, never as default performance
or end-to-end tail claims. See [handoff-attribution-review.md](handoff-attribution-review.md)
for the instrumented/default cross-checks and capacity-scan evidence.

The [channel publication review](channel-publication-review.md) preserves five
cancellation-safe out-of-lock prototypes and their rejection screens. Sampled
tail improvements did not compensate for uninstrumented cycle regressions; none
of those runtime changes is retained.

Heap allocation counts are independently available with `--features allocation-probe`. They cover
the measured process-wide interval and therefore should be collected with one worker on a quiet
machine. The TCP count also includes its native peer, so use the scheduler-only scenarios for clean
engine attribution. Rebuild without profiling features before recording headline latency results.

The [handoff review checkpoint](review-progress.md) records rejected optimization experiments,
migration-observation limits, and the expanded mutex soak qualification.
