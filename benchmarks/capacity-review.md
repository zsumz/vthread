# Provisioned-capacity review: 2026-09-05

The benchmark-only baseline is `778242d` on `perf/scheduler-hot-path`.
The runtime experiments below are rejected: a large park improvement
must not conceal a lifecycle regression. These are vthread baseline/candidate
comparisons, not new wins against May.

## Finding and candidate

With 64 live tasks, the historical harness configures `max_vthreads = 64`.
The new `--max-vthreads` argument changes provisioned admission capacity without
changing live tasks, queue limits, I/O capacity, stack cache, or measured work.
The effective capacity is printed. May rejects this vthread-specific option.

`WakeQueue::pending` scans `2 * capacity + 2` slots, each 32 bytes. At the runtime's
default 65,536-task capacity that is approximately 4 MiB per carrier, even when
only 64 tasks are live. `Kernel::snapshot` does this on progress publication.
`Shared::snapshot` later replaces that published remote depth with a fresh scan.

The observer-only candidate removes the redundant scan from carrier publication.
It publishes the local wake count and adds the freshly observed remote count when
an operator requests a snapshot. This also fixes the old replacement losing local
wakes. Deterministic tests failed before the repair: two carrier publications
performed two unwanted observations, and one local plus one remote wake appeared
as only one pending wake. All three targeted snapshot tests pass after the repair.

The queue, wake selection, generation retirement, memory orderings, carrier event
protocol, and allocation layout are unchanged. Explicit snapshot calls and cold
discard cleanup still scan slots; this is not an O(1) wake-mailbox implementation.
The local diagnostic component follows the existing 64-progress-event publication
bound (first mounts, completions, parks, and nonempty wake-drain batches). Ordinary
yields do not advance that bound; it is not a wall-clock freshness guarantee.

## Method and large-capacity evidence

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest presenting
an AMD EPYC 9555P. Benchmarks were serial and pinned to guest CPUs 0-3, without
concurrent compilation, qualification, or soak work. Guest topology does not prove
physical-host placement. The host has substantial elapsed-time variation.

The large-capacity park comparison used 10,000 operations per task, four carriers,
64 tasks, one warm-up, and nine measured rounds:

| Capacity | Baseline median ns/op | Observer candidate median ns/op |
| --- | ---: | ---: |
| 65,536 | 1,285.45 | 98.75 |

Both warm-ups reported all 32 pairs on distinct carriers. The approximately 13x
median improvement applies to this overprovisioned-capacity case, not all workloads.

Three hardware-counter repetitions each include process setup, a warm-up, and
three measured rounds: 2,560,000 park operations per invocation.

| Counter | Baseline mean | Observer candidate mean | Change |
| --- | ---: | ---: | ---: |
| Cycles | 28,374,963,900 | 2,237,553,536 | -92.1% |
| Instructions | 56,660,070,190 | 3,951,792,283 | -93.0% |

`perf` reported cycle variability of 0.61% / 4.06% and instruction variability of
0.95% / 0.51%, respectively. Counter access required a process-scoped escalation;
no kernel security or performance settings were changed.

## Small-capacity regression checks

A is the baseline, B the observer-only candidate. Each throughput invocation used
100,000 operations per task, four carriers, 64 tasks, capacity 64, one warm-up,
and nine measured rounds. Units are median ns/operation.

| Workload | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Park | 110.63 | 109.92 | 103.30 | 120.80 |
| Historical channel | 173.56 | 177.57 | 170.43 | 173.85 |

The mutex ordering was B/A/A/B: 414.57, 418.88, 419.96, 423.75 ns/operation.
The means of those paired invocation medians are approximately 419 ns for both
versions. Separate mutex counter means were 130,071,353,768 / 101,124,830,362 cycles
and 76,101,804,842 / 67,647,528,014 instructions (baseline/candidate). Cycle variability
was 4.15% / 10.66%; do not interpret that percentage reduction as a stable mutex win.

Wake-tail runs used 10,000 operations per task and 5,760,000 measured observations
per invocation, with the existing atomic timestamp slots and portable monotonic
clock. Values below are per-operation nanoseconds, not whole-round quantiles.

| Metric | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 241 | 230 | 240 | 241 |
| p99.9 | 149,777 | 147,312 | 141,614 | 146,672 |
| p99.99 | 160,882 | 157,808 | 154,253 | 164,088 |
| Worst-pair p99.9 | 150,528 | 150,066 | 148,033 | 151,118 |
| Task-median range | 220-270 | 200-261 | 210-290 | 220-271 |
| Maximum | 49,671,417 | 45,151,027 | 49,739,059 | 49,954,374 |

Yield medians were 15.73 / 15.52 ns (baseline/candidate) with four carriers and
64 tasks. These measurements do not establish the full tail/fairness matrix.

## Lifecycle regression and next experiment

The four-carrier, 10,000-task spawn/reclaim comparison remains a rejection gate.
An A/B/B/A sequence of 51-round invocations measured 432.41, 439.41, 463.52, and
425.49 median ns/task. Candidate admission was slower and drain faster. Whole-round
tails were wide and worse for the candidate in that sequence.

Longer counters confirmed a real cost. Three repetitions, each with a warm-up plus
501 measured 10,000-task rounds (5,020,000 lifetimes per invocation), measured:

| Counter | Baseline mean | Observer candidate mean |
| --- | ---: | ---: |
| Cycles | 22,399,815,118 | 35,310,008,662 |
| Instructions | 46,659,605,874 | 29,540,701,155 |

The cycle increase is approximately 57.6%, despite approximately 36.7% fewer
instructions. A separate 199-Hz cycle profile placed 37.50% of baseline samples in
`Kernel::snapshot`. In the candidate, 34.63% fell in the carrier-entry function;
its annotated samples concentrated around the idle `pause` loop. Removing the scan
changes admission/batching dynamics and exposes polling cost; merely deleting
instructions is not sufficient evidence of an end-to-end improvement.

Reducing admission-only idle polling to 64 probes did not recover lifecycle
performance: cycle means were 23,502,048,986 / 31,907,736,976 (baseline/candidate).
Removing admission-only busy polling was worse for elapsed time: candidate medians
were approximately 965-1,004 ns/task versus baseline 335-347 ns/task, and cycle means
were 24,065,824,920 / 29,584,076,185. Both variants are rejected.

Grouping admission-only probes into eight pauses per observation retained the
640-pause ceiling and the one-pause cadence for parked tasks, but still failed:
cycle means were 25,352,905,646 / 29,030,475,057 (baseline/candidate), approximately
14.5% higher. Candidate invocation medians were 400.56, 405.77, and 396.42 ns/task;
baseline medians were 356.15, 338.17, and 369.85. This variant is also rejected.
No idle-spin policy change is retained. The observer-only source experiment is
preserved locally for investigation of admission/batching before any retry.
The recovery object in this workspace is stash
`032c4563083b7d9c7c0f1fa7c971a3053f8ac3a0`; it is not a pushed runtime commit.

An earlier conditional pending-counter implementation was removed. It passed
tests and eliminated the capacity scan, but small-capacity mutex medians were
approximately 5.7% and 3.8% worse in reversed comparisons. No new queue counters
or their experimental model remain in the observer-only runtime.

## Reproduction and artifacts

Build immutable binaries from the baseline and each candidate; do not rebuild
while a timed invocation is running. Alternate order for elapsed-time comparisons.

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
taskset -c 0-3 BINARY vthread park 10000 4 64 9 --max-vthreads 65536
taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
taskset -c 0-3 BINARY vthread wake-tail 10000 4 64 9
taskset -c 0-3 BINARY vthread spawn 4 10000 51
perf stat -x, -r 3 -e cycles,instructions taskset -c 0-3 BINARY vthread park 10000 4 64 3 --max-vthreads 65536
perf stat -x, -r 3 -e cycles,instructions taskset -c 0-3 BINARY vthread spawn 4 10000 501
```

Baseline binary SHA-256:
`532f64e8f86033e4e9637316534192f1cc8bf529352f9028863e47ddedf33ebe`.
Observer-only binary SHA-256:
`97d5bbfd53ebd80f7d3c9f271ca56c62f1aa6a8a4b138a08b007546fd700f4a9`.
Local raw evidence uses `/tmp/vthread-observer-*.log`, `*.perf`, and `*.data`.
These are experiment artifacts, not a release-qualification bundle.

## Retained qualification and correctness work

The retained source checkpoint is `88bfdad`. It does not contain the capacity-scan,
pending-counter, or idle-polling experiments described above. Its production change
is a stall-detection repair in `7f2b3d8`: a terminal task record can precede retirement
of its scope's completion credit. The scope waiter previously treated that accounting
gap as an indefinitely parked scope and could publish an empty stall report or abort
the scope. A deterministic regression holds retirement open until the waiter blocks;
it failed on the old source with `terminal-only scope was reported: ReportAfter(0ns)`.
The repair requires a nonterminal task before declaring quiescence and passes the
regression for both report-only and aborting policies. The disabled-stall path and
real parked-scope detection remain intact. This is a correctness repair, not a speedup.

Two native-worker tests now isolate the outcomes they assert. Shutdown cleanup uses
ordered events while the sole native worker is gated, replacing a 100 ms scheduling
assumption. Worker-failure closure uses a separately cancellable five-second watchdog
after registration, replacing a scope deadline that could legitimately win first.
Neither change alters native-worker production code or weakens the expected result.

Wake-queue tests cover bounded observations at capacities 1 through 65,536, the
reserved-before-publication interval, rejected duplicates, and eight concurrent
producers reusing each route for 1,024 generations. They check every delivered notice,
exactly-once delivery, the outstanding bound, and complete drain. The mixed soak also
checks that all carrier wake queues are empty after shutdown. No queue atomic protocol
was changed.

Final source qualification passed:

- All 11 canonical `zcheck run check` gates, including 499 all-feature runtime tests,
  70 all-feature stack tests, documentation, application smoke, and zrail. Receipt:
  `/root/.cache/zcheck/run-1788580931-562123246-1941494/receipt.json`.
- Separate default-native `cargo test --locked --workspace --all-targets --quiet`:
  484 runtime tests, 66 stack tests, 11 synchronization-core tests, 19 lab tests,
  and one alias test.
- Standalone benchmark all-feature tests (27) and all-feature clippy with warnings
  denied. The architecture lock is unchanged.

The rebuilt native lab completed `soak 60 4 512`: 586 batches, 302,962 completed
lifetimes, 2,400,256 checked mutex updates, and 3,299,790 matching parks and wakes.
It allocated 516 stacks and reused 302,446; all shutdown drain assertions passed.
The run overlapped canonical qualification, so these totals are correctness evidence,
not throughput measurements. Lab binary SHA-256:
`e2b4e37cff39842d480d8cb303c0dd9a959eb38e1e650bef7931f82e739b717b`.

The source repair changes the rebuilt benchmark binary; it is not byte-identical to
the earlier baseline. Retained-source benchmark SHA-256:
`6a9b1b28d8e601f1270bb5e3a9898e55280325b34483515b97f8a55e4add56b8`.
These checks do not complete the modeled-protocol, sanitizer, architecture, fairness,
or ten-million-lifetime release matrix requested in the review.

### Retained-source performance checks

A is the immutable `778242d` benchmark; B is the rebuilt `88bfdad` source benchmark.
Serial A/B/B/A runs used guest CPUs 0-3, four carriers, capacity 64, and 64 tasks.
Park, historical channel, mutex, and yield used 100,000 operations per task, one
warm-up, and nine measured rounds. Spawn instead used 10,000 tasks and 501 measured
rounds, or 5,020,000 lifetimes including warm-up per invocation. No builds, tests,
or soaks ran concurrently with timing. Values are invocation medians in ns/operation.

| Workload | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Park | 92.37 | 91.78 | 94.46 | 94.45 |
| Historical channel | 139.73 | 133.97 | 139.10 | 145.36 |
| Mutex | 384.68 | 372.13 | 365.59 | 357.29 |
| Yield | 14.17 | 13.49 | 13.58 | 14.38 |
| Spawn/reclaim | 336.18 | 339.81 | 337.69 | 335.08 |

The baseline mutex median moved by about 7% between the bookends. These observations
do not establish a repair speedup. The spawn p99 values were 5,243.06, 5,289.33,
5,329.86, and 5,285.66 ns/task: these normalize whole-batch times, not individual
task-lifetime percentiles. Every paired park/channel warm-up retained 32 cross-carrier
pairs.

Wake-tail invocations used 10,000 operations per task and 5,760,000 measured samples
each. Values below are individual wake-to-resume nanoseconds.

| Metric | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 201 | 201 | 201 | 200 |
| p99.9 | 134,603 | 131,689 | 113,010 | 115,323 |
| p99.99 | 147,953 | 145,088 | 145,539 | 142,154 |
| Worst-pair p99.9 | 139,080 | 135,595 | 115,735 | 127,572 |
| Task-median range | 190-220 | 200-220 | 190-220 | 180-230 |
| Maximum | 49,678,145 | 53,275,365 | 84,986,130 | 49,840,051 |

The repair's 85 ms maximum outlier is retained in the evidence; aggregate percentiles
do not explain it or prove unchanged worst-case latency. This guest run is not a
completed tail/fairness qualification. No new May comparison or win is claimed.
Raw logs are `/tmp/vthread-stall-fix-{park,channel,mutex,wake-tail,yield,spawn}-{a1,b1,b2,a2}.log`.

Lifecycle counter groups also ran serially in A/B/B/A order, with three process
repetitions per group and 5,020,000 lifetimes per process including warm-up. Means
include process setup and all carrier work:

| Counter | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 24,444,363,985 | 24,524,819,181 | 23,678,575,300 | 24,689,941,151 |
| Instructions | 53,141,726,649 | 55,049,596,163 | 54,747,079,785 | 57,615,330,948 |
| Context switches | 10,116 | 7,924 | 4,351 | 4,304 |
| CPU migrations | 623 | 573 | 663 | 504 |

Reported cycle variability was 0.83%, 1.08%, 3.17%, and 1.96%, respectively. These
checks did not reproduce the rejected observer candidate's lifecycle cycle inflation;
they do not establish a correctness-repair performance win. Reproduce each group with
`perf stat -x, -r 3 -e cycles,instructions,context-switches,cpu-migrations taskset -c 0-3 BINARY vthread spawn 4 10000 501`.
Raw counter files are `/tmp/vthread-stall-fix-spawn-{baseline,repair}{,-reverse}.perf`,
with matching `*-counters.log` process output. Kernel settings were unchanged.
