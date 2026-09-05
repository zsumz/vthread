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

## Follow-up: cached public park binding

A candidate retained one public `Parker` binding per execution and used the existing
resident scheduler handoff. It reused the shared-primary cancellation lane, keeping
the internal synchronization lane separate. All 481 candidate native runtime tests
passed, including bounded fallback, storage reuse, and cancellation-race checks.
The candidate was nevertheless rejected and removed.

The same four-carrier A/B/B/A protocol produced baseline medians of 93.22 and 92.58
ns/operation, versus candidate medians of 101.73 and 95.82. Whole-round maxima were
95.61, 113.49, 160.65, and 106.26 ns/operation in that order. These are not
per-operation latency percentiles.

Three single-carrier counter repetitions (one warm-up plus three measured rounds
each) reduced mean cycles from 13,232,818,202 to 12,066,816,198, about 8.8%. Mean
retired instructions fell from 37,038,334,423 to 36,833,069,034. The locked-operation
total fell by approximately 25.6 million over 25.6 million operations per invocation.
The corresponding reversed-order four-carrier counter comparison increased cycles
from 20,206,950,121 to 21,096,993,101, about 4.4%, despite slightly fewer instructions.
Removing reference-count traffic did not establish the required multi-carrier win.

The lifetime audit also found a separate correctness defect in the retained baseline:
closing a semaphore or condition variable permanently closed a task's reusable wait
cell. A subsequent contended mutex acquisition failed with a scheduler fault; the
poisoned cell also survived execution-storage recycling. Deterministic scheduler
tests reproduced both failures before the repair. Gate-ticket cleanup now clears
only the private cell's closed bit, after generation retirement and synchronization
with the gate queue. It leaves the original primitive closed and preserves generation
identity. Word-state enumeration and racing delayed ready/cancel/timeout/close tests
cover the reset boundary. This is a correctness repair, not a claimed speedup.

The repair in `330f0d0` passed all 11 canonical gates under receipt
`/root/.cache/zcheck/run-1788570287-814788275-1876084/receipt.json`, with no architecture
lock changes. The separate default-native workspace suite passed, including 480
runtime tests, 66 stack tests, 11 synchronization-core tests, and 19 lab tests.
The 60-second four-carrier/64-worker native soak completed 264,684 lifetimes,
1,964,032 checked mutex updates, and 2,726,667 matching parks and wakes. It overlapped
native qualification, so its totals are correctness evidence only.

Serial four-carrier regression observations gave baseline/repair medians of
97.91/96.04 ns for park and 378.23/377.38 ns for mutex. Channel A/B/B/A medians were
149.11/156.95/161.77/149.47 ns. Investigating that apparent channel regression found
byte-identical `.text`, `.rodata`, and `.data.rel.ro` sections in the two benchmark
executables, with identical load-segment layouts. The `.text` SHA-256 was
`0565ba18406b15183b568cc70b80ad6e5231d03696d6e938b1407c72d4df6303` for both.
The repair is absent from the benchmark's generated hot paths. Single-carrier
channel counter means were 17,603,764,863/17,522,095,543 cycles and
50,250,592,504/50,251,712,257 instructions. These results demonstrate why elapsed-time
differences on this guest must not be attributed to code changes without supporting
evidence. The section inspection occurred after the reported baseline timings.

The atomic-clock wake-tail runs each collected 5,760,000 observations. Baseline/repair
median was 210/210 ns, p99.9 was 142,635/142,865 ns, and p99.99 was 157,157/156,256 ns.
Worst-pair p99.9 was 143,907/148,975 ns. These are regression observations, not a
new performance win or a completed tail-latency qualification.

## Follow-up: provisioned runtime capacity

The [capacity review](capacity-review.md) records a newly exposed spare-capacity
cliff, hardware-counter and idle-loop profiling evidence, and rejected counter,
snapshot, and idle-polling experiments. The benchmark control is retained;
none of those runtime changes is retained because lifecycle regression checks failed.

## Follow-up: owner wake dequeue

The [dequeue review](dequeue-review.md) records cycle profiles, an unretained forced-inline
dequeue experiment, and its complete reversed-order counter and tail evidence. It also
records May's default OS-worker pinning and strengthens the foreign-runtime Drop test
with explicit event ordering. The next protocol experiment targets the owner wake-head
exchange; no mailbox implementation or corresponding speedup is claimed here.
