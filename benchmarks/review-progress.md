# Handoff review checkpoint: 2026-09-05

The runtime baseline for these experiments is `9835e61` on
`perf/scheduler-hot-path`. None of the three runtime experiments below is retained.
The retained work adds benchmark migration observations, release-race and panic-handoff
regressions, and contended mutex traffic in the existing mixed-workload soak.

## Benchmark interpretation

May's warm-up observer now covers park, channels, and mutexes. In one four-worker,
64-task mutex warm-up, 31 tasks changed execution workers. Earlier park and channel
warm-ups also observed migration and final pair co-location. Vthread's corresponding
paired warm-ups retained 32 cross-carrier pairs.

These are instrumented warm-up observations, not migration counts for the timed
rounds. Final pair placement does not describe every handoff. They establish that
equal worker counts do not establish equal execution topology; they do not quantify
how much of the measured performance difference migration causes. Measured May
closures use compile-time-disabled observers.

## Rejected runtime experiments

Runs used Rust 1.96.1, LLVM 22.1.2, Linux 5.15.0-187-generic, and an eight-vCPU KVM
guest reporting an AMD EPYC 9555P. Four-carrier workloads were restricted to guest
CPUs 0-3; hardware-counter runs used guest CPU 7. Guest CPU topology does not establish
physical-host placement. The host has visible timing variation, so these numbers
are experiment evidence rather than portable performance claims.

Each throughput invocation performs one warm-up and nine measured rounds, with
100,000 operations per task and 64 tasks. The table records sequential A/B/B/A
invocations. A is the baseline; B is the candidate. Units are ns per operation.

| Experiment | A median | B median | B median | A median | Decision |
| --- | ---: | ---: | ---: | ---: | --- |
| Load waiter bit before unlock CAS | 369.49 | 384.72 | 392.26 | 378.05 | Slower in both orders |
| Return handed-off guards directly through queue | 378.05 | 377.24 | 394.69 | 394.62 | No repeatable four-carrier gain; mixed single-carrier tradeoff |
| Keep channel miss and wait registration under one lock | 146.93 | 150.80 | 142.89 | 151.00 | Unstable improvement and worse round tail |

The channel whole-round p99 values, normalized per operation, were 164.54, 169.42,
233.45, and 170.00 in the same order. With nine samples, this p99 is the observed
maximum round. It is not a per-operation p99 or p99.9.

For direct guard handoff, three single-carrier counter runs showed:

| Workload | Baseline cycles | Candidate cycles | Change |
| --- | ---: | ---: | ---: |
| Forced mutex contention | 22,001,312,142 | 21,647,145,999 | -1.6% |
| Uncontended mutex | 3,323,921,751 | 3,381,926,272 | +1.7% |

The contended workload retired approximately 25.6 million fewer locked operations
over 25.6 million acquisitions: the intended redundant CAS was removed. Uncontended
instructions nevertheless increased from 7,746,265,636 to 8,046,208,410 (+3.9%). The
tradeoff was insufficient to retain a new guard field and release branch.

## Commands

Build separate binaries from the baseline and each candidate. Do not rebuild or run
other benchmarks concurrently with a timed invocation. Alternate order across rounds.

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
taskset -c 0-3 BINARY vthread channel 100000 4 64 9
perf stat -x, -r 3 -e cycles,instructions,ls_locks.spec_lock_hi_spec,ls_locks.spec_lock_lo_spec,ls_locks.non_spec_lock taskset -c 7 BINARY vthread mutex 100000 1 64 3
perf stat -x, -r 3 -e cycles,instructions taskset -c 7 BINARY vthread mutex-uncontended 10000000 1 1 5
```

Counter totals include the warm-up and process setup. These measurements reject
candidates; they are not evidence that the remaining May targets have been met.

## Qualification scope

The mixed soak now holds a vthread mutex across eight yields per worker, checks the
exact shared update count, verifies the waiter queue drains, and reports the checked
update total. It continues exercising channels, TCP, timers, semaphore grants,
native jobs, cancellation, stack reuse, and shutdown.

The default native-engine release binary completed these 60-second mixed soaks:

| Carriers | Workers per batch | Batches | Completed lifetimes | Checked mutex updates | Parks and wakes |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4 | 64 | 5,493 | 379,017 | 2,812,416 | 3,888,216 each |
| 1 | 64 | 4,784 | 330,096 | 2,449,408 | 3,496,323 each |

Both runs checked exact payloads, unchanged task affinity, empty waiter queues,
complete task reclamation, and drained native/readiness services. They overlapped
other qualification activity; these totals are correctness evidence, not throughput
benchmarks. Reproduce with:

```sh
cargo build --release --locked -p vthread-lab --bin vthread-lab
target/release/vthread-lab soak 60 4 64
target/release/vthread-lab soak 60 1 64
```

The concurrent canonical run exposed a pre-existing sleep-test assumption: a worker
was required to execute before a 1 ms sleep expired. Valid host preemption produced
`sleep:start, sleep:end, worker`. The replacement drives the scheduler and the exact
timer generation explicitly; a separate real-clock check verifies the minimum sleep
duration. No runtime timer policy was changed to accommodate the assertion.

These checks do not substitute for the full modeled-protocol, sanitizer, architecture,
tail-latency, and ten-million-lifetime qualification proposed in the review.

The source checkpoint `3042f08` passed all 11 canonical `zcheck run check` gates,
including zrail, under receipt
`/root/.cache/zcheck/run-1788568140-313600468-1856979/receipt.json`.
`cargo test --locked --workspace --all-targets` separately passed with the default
native engine, including 475 runtime and 66 stack-engine tests. The standalone
benchmark suite passed all 24 tests and its all-features clippy check.
