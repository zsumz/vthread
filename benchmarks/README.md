# Scheduler benchmarks

This standalone harness exercises the `vthread` scheduler with structured scopes
and 64 KiB stacks. It reports whole-round timings and optional workload-specific
latency and diagnostic observations.

## Quick start

Build once and run each workload in a fresh process:

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/vthread-benchmarks vthread yield 100000 1 1 11
```

<details>
<summary>More workload examples</summary>

```sh
benchmarks/target/release/vthread-benchmarks vthread spawn 1 1000 11
benchmarks/target/release/vthread-benchmarks vthread park 100000 1 2 11
benchmarks/target/release/vthread-benchmarks vthread mutex 100000 1 2 11
benchmarks/target/release/vthread-benchmarks vthread mutex-uncontended 1000000 1 1 11
benchmarks/target/release/vthread-benchmarks vthread channel 100000 1 2 11
benchmarks/target/release/vthread-benchmarks vthread channel-bounded-spsc 100000 64 1 2 11
benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 20000 64 4 64 11
benchmarks/target/release/vthread-benchmarks vthread tcp 1000 1 1 11
benchmarks/target/release/vthread-benchmarks vthread wake-tail 10000 1 2 11
```

</details>

The literal `vthread` command prefix is required in every invocation. Arguments
after the scenario are operations per task, workers, tasks and samples.
`spawn` omits operations per task. `channel-bounded-spsc` and `channel-mpmc` insert
a positive channel capacity immediately after operations per task. Every numeric
argument must be positive; the sample count must be odd.

## Workload contracts

| Scenario | Reported operation | Task requirement |
| --- | --- | --- |
| `yield` | One cooperative yield | Any positive count |
| `spawn` | One admitted, completed, reclaimed and drained task | Any positive count |
| `park` | One half of a paired park/unpark handoff | Even count, at least two |
| `mutex` | One contended lock acquisition and release | At least two |
| `mutex-uncontended` | One immediately available lock acquisition and release | Exactly one |
| `channel` | One message handoff in a paired capacity-one exchange | Even count, at least two |
| `channel-bounded-spsc` | One message handoff in a paired exchange at the requested capacity | Even count, at least two |
| `channel-mpmc` | One value transferred through one shared bounded channel | Even count, at least four |
| `tcp` | One write/read echo round trip on a task-owned connection | Any positive count |
| `wake-tail` | One timestamped wake-to-resume handoff | Even count, at least two |

Each paired channel uses the general bounded channel with one producer and one
consumer per direction.

With one worker, `mutex` yields while holding the lock to force handoffs. With
multiple workers, it performs 32 black-box operations in each critical section.

TCP requires local socket creation. The native echo peer's startup and shutdown
are inside the measured interval.

### Shared MPMC channel

`channel-mpmc` splits tasks equally into producers and consumers. Each producer
sends distinct identifiers, and each consumer receives the requested quota.
The denominator is
`tasks / 2 * messages-per-producer`: each transferred value is counted once.
The waiter bound per direction equals the number of producers. Task-local receiver
vectors are allocated and recorded inside the measured interval.

Every round's count, identifier range and uniqueness are checked after timing and
allocation recording stop.

## Capacity and carrier pinning

Runtime admission capacity defaults to the larger of the task and worker counts.
Append `--max-vthreads <capacity>` to provision additional capacity without changing
the live workload. The limit must cover both counts and is printed in the output.

On Linux, `--pin-carriers` pins each carrier to a distinct allowed CPU and verifies
the resulting thread mask. It requires procfs, `taskset` and enough allowed CPUs.
Printed pinning ranks order OS thread IDs; they are not runtime carrier IDs.

```sh
taskset -c 0-3 benchmarks/target/release/vthread-benchmarks vthread park 10000 4 64 9 --max-vthreads 65536 --pin-carriers
```

## Timing and reports

Every run has one untimed warm-up and an odd number of measured rounds. Runtime
construction and carrier pinning happen before warm-up. Each measured interval
includes scope creation, task admission, operations, completion, joining and drain.

Whole-round output includes median, p95, p99, maximum and all round samples.
`tcp` and `wake-tail` also report measured per-task latency streams through p99.99,
including task and pair tail spread where applicable. Warm-up observations are
excluded from timing results.

Paired park, channel and wake warm-ups report immutable owner-carrier pairs;
measured rounds collect no placement snapshots.

### Optional channel latency sampling

Append `--sample-channel-latency` to `channel-mpmc` for separate send and receive
API-call distributions, with one timestamp interval per call and task. The operation
label gains `-sampled`; each transferred value still counts once and supplies two
endpoint observations. Logical streams are consumers first, then producers, across
rounds.

```sh
benchmarks/target/release/vthread-benchmarks vthread channel-mpmc 2000 1 4 64 3 --sample-channel-latency
```

Clock reads and recording are inside timing; validation and summarization are
outside. These closed-loop distributions describe API calls and stream spread;
they do not establish arrival-to-delivery latency or a starvation bound.

## Diagnostic features

Optional Cargo features provide diagnostic attribution:

| Feature | Additional observations |
| --- | --- |
| `allocation-probe` | Process-wide allocation, deallocation and requested-byte counts during measured intervals |
| `lifecycle-profiling` | Admission, materialization, reclaim, completion and retirement timings |
| `scheduler-profiling` | Carrier activity counters read only after shutdown, including startup and warm-up |
| `handoff-profiling` | Channel and mutex progress, wake routing and overlapping duration histograms; implies `scheduler-profiling` |

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml --features handoff-profiling
benchmarks/target/release/vthread-benchmarks vthread mutex 1000 4 64 3
```

Profiling changes execution cost and layout. Keep instrumented observations
separate from default-build timings; overlapping durations cannot be added as CPU
time. Allocation counts include the native TCP peer and other process activity.

## Recording comparable results

Record source and binary identities, feature flags, CPU affinity and host conditions
with results. Use a quiet host: carrier placement, frequency scaling and scheduling
noise affect short and multicarrier runs. Rebuild without profiling features when
collecting default-runtime timings.
