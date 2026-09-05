# Pending-admission fairness

Baseline: `a456324`, with the same production runtime as `3372486` and the ordered
stall/service-publication tests qualified separately. This slice changes
only pending-admission credit and the quota-expiry batch. Ready ordering, wait-word
publication, wake routing, placement, capacity scans and idle pacing are unchanged.
Retained as a qualified bounded-progress repair with measured costs, not a
throughput optimization. The production-sized capacity cliff remains open.

## Real-kernel negative controls

Three new tests failed before the production change:

- With 64 resident tasks mixing yields, actual parks and externally routed wakes,
  one late start was still pending after 65,536 dispatches. The ready queue stayed
  above its refill threshold throughout. Failure: `parking erased admission progress`.
- The same failure occurred with 64 borrowed children and their structured parent.
  This uses real native fibers and the actual kernel, not a separate policy model.
- At quota expiry with 128 packets behind the 64-task window, one receive grew
  ready occupancy to 192 rather than 65. Failure: `unbounded quota batch`.

The last failure describes materialization batching, not a breach of the runtime's
global capacity. The old code drained up to configured carrier queue capacity.

## Repair and dispatch contract

The existing carrier-local counter now counts admission checks while remote backlog
is known. The carrier checks once per drive iteration, regardless of whether the
previous task yielded, parked, completed or was discarded. Parks and completion no
longer reset it. Only actual ingress removal or observing an empty backlog clears
credit. No new counter, atomic or allocation is added. Accounting lives in receive,
so task dispatch does no quota work when admission is absent.

Ordinary refill still targets at most 64 ready entries (or configured queue capacity
if smaller), when ready occupancy is at or below half the target. A continuously
full window admits **one** additional FIFO start after 65,536 dispatch opportunities.
The carrier checks receive before the next dispatch while backlog remains. A packet
with N older queued packets therefore receives service within at most
65,536 * (N + 1) dispatch opportunities after backlog is observed, assuming the
carrier continues driving and admission is not shut down. Allocation failure is a
tracked terminal outcome, not a successfully started task.

Admitted starts join the normal ready queue and receive its separate cooperative
selection bound. These bounds are not wall-clock promises for non-yielding code,
nor a claim that a 65,536-dispatch admission latency is the ultimate tuned policy.
The large quota is deliberately unchanged in this correctness slice.

The mixed tests verify late execution within the ready bound, thousands of actual
parks/yields, capacity and unchanged OS-thread identity across resumes. Cleanup
reclaims real borrowed scopes before reporting any negative-control assertion.
Additional coverage verifies completion preserves credit, quota service stays at
one packet, an empty backlog does not precharge credit, bounded ordinary refill,
yield-only progress and pending-ingress avoidance of native sleep.

## Qualification and measurement

The initial admission tree passed all 11 canonical gates under receipt
`/root/.cache/zcheck/run-1788628442-967737575-2165438/receipt.json`.
Its following default-native run failed the late-service 200 ms test. Performance
measurement was paused and the patch preserved while that failure and the earlier
root-stall failure were captured and their test ordering repaired in `a456324`.
See [wait-qualification-review.md](wait-qualification-review.md) for the actual
failure states, negative control and additional open oversubscription findings.
The architecture-lock refresh adds the test module to source inventory only:
zero grants, revocations, dependencies or ratchets. No concurrent atomic protocol
changed, so these tests exercise the actual carrier-local state directly.

Final-tree canonical qualification passes all 11 gates under receipt
`/root/.cache/zcheck/run-1788630999-718141630-2275285/receipt.json`.
Default-native workspace tests pass: 505 runtime tests (one ignored manual probe),
70 stack tests, and all remaining model/lab/reexport suites. Standalone benchmark
tests (32) and all-target clippy pass. Sequential optimized 60-second native mixed
soaks complete 261,855 lifetimes on one carrier and 339,549 on four: all 601,404
lifetimes complete, parks/wakes balance, and service/shutdown assertions pass.

The preserved negative-control log predates the completion-credit test; the other
three new tests demonstrably failed the old policy. The initial repaired prototype
counted in dispatch; the final version counts the equivalent carrier-loop admission
opportunities in receive, avoiding quota accounting on no-backlog task paths.

## Serial performance panel

A is the immutable cohort-2 baseline, C the admission repair. Each row is four fresh
processes in A/C/C/A order, with no overlapping builds, tests, soaks, profiling,
disassembly or other benchmarks. Values are invocation-median throughput-derived
ns/operation, not individual latency. The change column compares the means of two
process medians, not a confidence interval.

Linux 5.15.0-187-generic, Rust 1.96.1 / LLVM 22.1.2, eight-vCPU KVM guest reporting
AMD EPYC 9555P. Four-carrier process mask 0-3, carriers individually unpinned except
where explicitly stated. One-carrier processes use CPU 7. Host placement and load
are unknown. Normal capacity equals task count unless an override is shown.

| Workload | A1 | C1 | C2 | A2 | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Yield, 4 / 64 | 15.63 | 15.56 | 15.76 | 14.92 | +2.5% |
| Lifecycle, 4 / 1,000 | 395.62 | 396.66 | 395.89 | 394.70 | +0.3% |
| Lifecycle, 4 / 10,000 | 337.97 | 341.39 | 336.86 | 337.50 | +0.4% |
| Park, 4 / 64 | 92.22 | 105.64 | 108.04 | 94.04 | +14.7% |
| Mutex, 4 / 64 | 362.23 | 357.00 | 361.29 | 375.17 | -2.6% |
| Historical channel, 4 / 64 | 143.24 | 145.59 | 140.17 | 142.36 | +0.1% |
| Timestamped wake throughput, 4 / 64 | 142.95 | 136.51 | 138.08 | 133.94 | -0.8% |
| Uncontended mutex, 1 / 1 | 27.46 | 22.77 | 22.75 | 22.75 | -9.3%* |
| Park, 4 / 1,000 | 104.18 | 106.54 | 99.53 | 103.68 | -0.9% |
| Park, 4 / 64, capacity 1,024 | 132.67 | 135.82 | 135.74 | 101.00 | +16.2%* |
| Park, 4 / 64, capacity 65,536 | 1,173.55 | 1,219.74 | 1,149.81 | 1,088.80 | +4.7% |
| Lifecycle, 4 / 1,000, capacity 65,536 | 1,522.28 | 1,545.62 | 1,600.30 | 1,644.29 | -0.7% |

The starred rows show substantial within-baseline variation. In particular, the
last uncontended baseline matches the candidate; the apparent mean win is not
claimed as a reproducible optimization. The spare-capacity cliff remains in both
binaries and is not repaired by this admission change.

Each wake invocation records 5,760,000 atomic-timestamp observations:

| Individual wake statistic, ns | A1 | C1 | C2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 241 | 231 | 230 | 231 |
| p95 | 10,306 | 10,487 | 10,616 | 10,456 |
| p99 | 12,008 | 12,269 | 13,491 | 12,088 |
| p99.9 | 15,314 | 15,273 | 15,483 | 15,102 |
| p99.99 | 27,521 | 25,247 | 27,271 | 25,298 |
| Worst-pair p99.9 | 15,924 | 16,675 | 16,474 | 15,844 |
| Maximum | 53,471,109 | 49,859,013 | 49,451,069 | 49,571,721 |

Stored permits can contribute samples. Closed-loop ping-pong, warm-up placement
observations and these two process invocations do not establish loaded-tail or
measured-round topology guarantees. The earlier cohort repair's approximate
15-17 us p99.9 remains, with modest variation; no new tail win is claimed here.

## Park follow-up and counters

The initial +14.7% park result was investigated before retention. Fresh serial
A/C/C/A repeats used the same workload, first with explicit carrier pinning and
then with production placement/unpinned carriers:

| Park, 4 / 64, ns/operation | A1 | C1 | C2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Explicitly pinned | 104.27 | 110.72 | 93.57 | 93.13 |
| Unpinned repeat | 91.29 | 92.95 | 95.62 | 92.51 |

The 15% gap did not persist; the repeated unpinned mean cost was 2.6%. The pinned
invocations also varied materially, so this is not claimed to identify the cause.

A final reversed-order C/A/A/C panel gives the following invocation medians:

| Workload, ns/operation | C1 | A1 | A2 | C2 |
| --- | ---: | ---: | ---: | ---: |
| Pinned park, 4 / 64 | 109.35 | 106.90 | 92.86 | 94.64 |
| Pinned yield, 4 / 64 | 15.59 | 15.39 | 14.01 | 13.38 |
| Uncontended mutex, 1 / 1 | 27.56 | 27.53 | 27.60 | 22.89 |

The reversed park comparison costs 2.1% by process-median means. These runs support
a small park cost, not the initial 15% estimate and not zero regression. The other
rows reinforce why isolated invocation improvements must not become speedup claims.

Counters include process setup and one warm-up plus three measured rounds,
25,600,000 operations per process, again A/C/C/A:

| Counter | A1 | C1 | C2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| One-carrier cycles | 13,440,208,349 | 13,534,388,537 | 13,496,180,796 | 13,435,572,016 |
| One-carrier instructions | 37,068,575,346 | 37,044,225,460 | 37,044,145,123 | 37,069,735,995 |
| Four-carrier pinned cycles | 20,997,788,127 | 20,868,559,384 | 21,461,426,871 | 21,284,591,440 |
| Four-carrier pinned instructions | 39,399,979,972 | 39,393,502,481 | 39,480,456,016 | 39,469,398,814 |
| Four-carrier context switches | 940 | 969 | 926 | 735 |
| Four-carrier CPU migrations | 61 | 68 | 63 | 55 |

Single-carrier cycles/operation are 524.92 versus 527.94 (+0.58%), with instructions
down 0.07%. Pinned four-carrier cycles are 825.83 versus 826.76 (+0.11%), instructions
within 0.01%. These do not demonstrate a cycle reduction. Migration counts cover
the entire process, including auxiliary workers; they are not counts of steady-
state pinned-carrier migration or started-task migration.

## Commands and identity

```sh
timeout 120s taskset -c 0-3 BINARY vthread yield 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread spawn 4 1000 101
timeout 120s taskset -c 0-3 BINARY vthread spawn 4 10000 101
timeout 120s taskset -c 0-3 BINARY vthread park 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread channel 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread wake-tail 10000 4 64 9
timeout 120s taskset -c 7 BINARY vthread mutex-uncontended 10000000 1 1 9
timeout 120s taskset -c 0-3 BINARY vthread park 10000 4 1000 5
timeout 120s taskset -c 0-3 BINARY vthread park 10000 4 64 9 --max-vthreads 1024
timeout 120s taskset -c 0-3 BINARY vthread park 10000 4 64 9 --max-vthreads 65536
timeout 120s taskset -c 0-3 BINARY vthread spawn 4 1000 101 --max-vthreads 65536
timeout 120s taskset -c 0-3 BINARY vthread park 100000 4 64 9 --pin-carriers
timeout 120s perf stat -x, -e cycles,instructions -o COUNTERS.csv \
  taskset -c 7 BINARY vthread park 100000 1 64 3
timeout 120s perf stat -x, -e cycles,instructions,context-switches,cpu-migrations \
  -o COUNTERS.csv taskset -c 0-3 BINARY vthread park 100000 4 64 3 --pin-carriers
```

The source/environment manifest, immutable executables, raw output and patch are
in `target/finish-line/admission`. Candidate source SHA-256:
`fc9156a9b98f1c0dda646c028cf853e101e3edf8c7c269dc4b4b20732542ba6e`.
Baseline executable: `0ff35577c7b29e97bee1cf9bcc64cf0ba4e7b4ed4233defc6031fc9fe7f7de95`.
Candidate executable: `2f4d1623c9e0f7a1d37cff897794907a363d251fea5cb797f638d2448c7283d5`.

Raw output, source patch, environment/binary hashes and qualification receipt/logs
are committed in
[`evidence/admission-fairness-fc9156a9.tar.gz`](evidence/admission-fairness-fc9156a9.tar.gz),
SHA-256 `398d1233d2e195d542768037c64bb4ecb0fb21ec8c7f1c0f84f55cfd720e4850`.
Executed binaries remain local; this is slice evidence, not a full release bundle.

## Retention decision

Retain under the review's explicit fairness/correctness tradeoff: the old protocol
can indefinitely postpone a start under bounded mixed traffic, including borrowed
work, and can materialize the whole queued backlog when its quota expires. The new
protocol closes both failures without altering task affinity, generation selection,
scope ownership or shutdown admission. It adds no shared atomic or unbounded queue.

The measured park cost is roughly 2-3% in follow-ups; isolated cycles cost 0.58% and
pinned four-carrier cycles 0.11%. Small tail and worst-pair differences are reported
above, not claimed neutral. This is not acceptance under a cycle-reduction claim.
Lifecycle remains close to baseline, the earlier capacity cliff persists, and the
broader oversubscribed-suite findings from the separate test review remain open.
No throughput optimization, May win or release-ready claim is made by this repair.
