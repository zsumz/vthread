# Quiet-window checkpoint: 2026-09-06

Vthread's execution/lifecycle wins survive a fresh default-build comparison.
Contended coordination, wake tails and spare-capacity costs remain unfinished.
**This is measurement and evidence, not a new runtime optimization or release.**
Nine counter-free workload comparisons completed; the park control remains
unqualified because two of its four May processes timed out. Those failures are
retained, not replaced or credited as a vthread performance win.

The pristine measured checkout is `bfdef5e`, with runtime/test/harness source
identical to qualified `462c636`. It includes the necessary publisher-progress
repair and preserved fairness guarantees. No engine, checkpoint, ownership,
placement, admission, channel or polling policy changed during this window.

## Default-contract throughput

Four fresh processes per engine/case, two engine orders each, case order reversed
on even repetitions. Cells are medians of process-level round medians; parentheses
give the range of those four medians. Units are ns/operation or ns/completed-and-
drained task. These are throughput-derived times, not individual event latencies.

| Workload | Vthread | May 0.3.51 | Direction |
| --- | ---: | ---: | --- |
| Yield, 4 carriers / 64 tasks | 13.65 (13.62–15.84) | 17.05 (15.78–21.75) | Vthread 1.25x faster |
| Lifecycle, 4 / 1,000 | 392.96 (381.02–411.54) | 2,920.08 (2,884.24–3,356.17) | Vthread 7.43x faster |
| Lifecycle, 4 / 10,000 | 358.79 (334.50–365.17) | 5,511.81 (5,473.20–5,983.06) | Vthread 15.36x faster |
| Contended mutex, 4 / 64 | 423.76 (409.89–427.70) | 159.74 (159.35–159.97) | May 2.65x faster |
| Uncontended mutex, 1 / 1 | 22.73 (22.73–27.44) | 40.04 (40.02–40.04) | Vthread 1.76x faster |
| Historical paired channel, 4 / 64 | 162.93 (159.06–170.04) | 71.34 (70.62–71.80) | May 2.28x faster; unequal contracts |
| Capacity-one SPSC control, 4 / 64 | 165.31 (162.35–168.53) | 24.70 (23.71–26.22) | May 6.69x faster; narrower May primitive |
| Timestamped wake throughput, 4 / 64 | 149.24 (141.81–159.15) | 96.37 (92.29–104.55) | May 1.55x faster |
| TCP whole-round throughput, 4 / 8 | 14,172.51 (13,263.91–16,301.58) | 12,390.98 (11,796.98–12,820.40) | May 1.14x faster |

Counter-free vthread park is 109.56 ns/op (109.04–115.81). Only two May park
processes produced timing, 55.53 / 55.57 ns/op; two exited 124 at the planned
180-second limit. **There is no completed four-process counter-free park ratio.**
The separately instrumented panel completed all four processes per engine:
108.47 / 62.60 ns/op, favoring May by 1.73x under that collection mode. It does
not fill the missing counter-free observations.

Both engines use 64 KiB stacks. Vthread capacity matches the task count. Runtime
construction is outside round timing; scope/task/completion/join work is inside.
Each process has one warm-up and nine measured rounds, except lifecycle's 101.
Yield/handoff work is 100,000 operations/task, uncontended mutex ten million,
wake-tail 10,000/task and TCP 1,000/task. Four-worker cases use guest CPUs 0–3;
uncontended mutex uses CPU 7. Actual commands/configuration are archived.

May retains default individual-worker pinning and coroutine migration. Vthread
is not individually pinned and preserves post-mount affinity. Warm-up pair
observations are not measured-round topology. The historical channel compares
bounded-one vthread with unbounded May MPSC. The capacity-one control compares
vthread's general bounded channel with May SPSC plus semaphore; it does not
equalize API, cancellation, fairness, migration or allocation contracts.

## Individual latency and tails

Four counter-free processes per engine supply 5,760,000 wake observations each.
The table reports medians of their quantiles; maximum is the worst observation
across processes. Values are ns. This is four independent repetitions, not
millions of independent experiments.

| Wake statistic | Vthread | May |
| --- | ---: | ---: |
| p50 | 261 | 666 |
| p95 | 12,334 | 1,807.5 |
| p99 | 14,782 | 2,068 |
| p99.9 | 17,821.5 | 3,490 |
| p99.99 | 39,364.5 | 7,927 |
| Worst-pair p99.9 | 19,254 | 4,216 |
| Maximum | 49,478,916 | 61,850,113 |

Vthread's median is 2.55x better, but p99.9 is 5.11x worse. Its narrower task-median
spread is not a fairness proof. The smaller p99.9 than the historical 113–144 us
panel is consistent with the separately qualified fairness repair; this run is
not a paired old/new attribution experiment. Stored permits need not include a
full suspension, and closed-loop ping-pong does not measure offered-load delay.

Each TCP process supplies 72,000 individual one-byte loopback round trips:

| TCP statistic, ns | Vthread | May |
| --- | ---: | ---: |
| p50 | 19,189 | 21,167.5 |
| p99 | 136,100 | 160,587 |
| p99.9 | 21,049,236.5 | 4,823,824.5 |
| p99.99 | 67,697,486.5 | 51,464,257.5 |
| Maximum | 95,770,636 | 81,930,333 |

Vthread's median is slightly better, but whole-round throughput and deep tails
favor May. Eight connections are not readiness-scale qualification. This is not
HTTP. The separate [offered-load harness](offered-load-review.md) is qualified,
but its controlled-host tail/CPU acceptance remains outstanding.

## Counter collection changes the result

The initial 80-process `perf stat` panel completed with all guards clear and all
five events available at at least 99.5% coverage. Its lifecycle timing suggested
25–40x vthread advantages. **Those are not the default timing headlines above.**
A separate 24-process crossover reverses collector order for each engine/case:
two fresh processes per collector/cell, identical executable, arguments and mask.

| Crossover cell | No-perf ns/op | With-perf ns/op | With/without |
| --- | ---: | ---: | ---: |
| May lifecycle, 1,000 | 3,161.78 | 10,648.31 | 3.37x |
| May lifecycle, 10,000 | 5,294.37 | 13,739.59 | 2.60x |
| Vthread lifecycle, 1,000 | 398.40 | 416.23 | 1.04x |
| Vthread lifecycle, 10,000 | 331.61 | 335.62 | 1.01x |
| May TCP | 12,450.57 | 34,463.91 | 2.77x |
| Vthread TCP | 14,272.55 | 41,977.84 | 2.94x |

The crossover establishes collection-sensitive timing on this guest, not the
underlying PMU/kernel mechanism or a general statement about other machines.
No runtime tuning follows it. The [mutex controls](mutex-mechanism-quiet-review.md)
also retain separate counter-free results: 191 ns local, 311 ns remote-active,
3.67 us after observed sleep, versus 10.57 us sleep with counters.

The main counter panel's whole-process cycles per operation remain useful,
separately labeled evidence:

| Case | Vthread cycles/op | May cycles/op |
| --- | ---: | ---: |
| Yield | 117.88 | 166.69 |
| Lifecycle, 1,000 | 6,027.60 | 22,369.14 |
| Lifecycle, 10,000 | 4,924.45 | 22,880.21 |
| Park | 950.04 | 561.65 |
| Contended mutex | 4,446.28 | 720.90 |
| Uncontended mutex | 55.35 | 92.31 |
| Historical channel | 1,437.69 | 651.32 |
| Capacity-one SPSC | 1,477.29 | 227.39 |
| Timestamped wake | 1,411.12 | 1,042.68 |
| TCP | 213,429.50 | 156,222.46 |

These include setup, warm-up, native waiting/polling, reporting and shutdown;
the denominator includes warm-up operations. They are not isolated switch, lock
or I/O cycles. Task-clock, instructions, context switches and migrations are also
archived. Do not combine these perturbed costs with bare timings as if from one run.

## Spare capacity remains costly without counters

These are separate vthread-only controls, four fresh processes per cell, normal
placement, unchanged polling/refill policy. Shared MPMC transfers use 32 producers
and 32 consumers, capacity-one channel, 2,000 values/producer, three measured
rounds; each value is counted once. Park uses 64 tasks, 2,000 operations/task,
three rounds. Lifecycle uses 1,000 tasks and 101 rounds.

| Workload | Tight task capacity | Capacity 1,024 | Capacity 65,536 |
| --- | ---: | ---: | ---: |
| Shared channel, ns/value | 1,286.40 | 1,409.25 | 15,522.57 |
| Park, ns/op | 82.30 | 64.00 | 1,412.71 |
| Lifecycle, ns/task | 385.73 | Not run | 1,644.03 |

Tight capacity is 64 for synchronization and 1,000 for lifecycle. Channel process
medians span 864–1,463 ns at capacity 64 and 9,044–25,938 ns at 65,536; park's tight
range is 58–163 ns. Those wide spreads are not hidden. All four spare-capacity
processes still lose substantially. Separate counter-collected controls also
complete, but are not pooled here. This confirms baseline capacity sensitivity,
not a new fix, a complete footprint study or equivalent May admission semantics.

## Progress finding, qualification and durable evidence

The first bare May park process and repetition three each consumed roughly four
guest CPUs without completing within 180 seconds. Both endpoint guards were clear.
The first backtrace attempt could not run because gdb was absent. A short sample
attached late to repetition three captures scheduler/yield activity; exact data,
decoded stacks and limitations are retained. It does not classify the cause.
The latter is explicitly diagnostic, never a valid bare timing observation.
Successful repetitions two/four cannot explain or erase either failure.

Inventory: 80 successful counter comparisons, 78 successful/two failed bare
comparisons, 32 counter capacity processes, 32 bare capacity processes and 24
crossover processes. All 248 planned invocations ran, with 246 successes and two
timeouts. Every build guard is clear. The resumed runner consumed only untouched
planned jobs; no replacement samples or silently omitted repetitions.

Host: eight-vCPU KVM guest reporting AMD EPYC 9555P, Linux 5.15.0-187, Rust 1.96.1 /
LLVM 22.1.2, no RUSTFLAGS. Endpoint guards do not prove physical-host isolation,
fixed frequency or absence of VM scheduling tails. Ratios are guest/protocol-
specific. Whole-round samples and endpoint quantile/fairness reports are durable;
this harness does **not** export individual raw wake/TCP samples. The mutex fixture
does. This distinction remains a release-evidence limitation.

The unchanged source matches the preceding fourteen-gate canonical receipt,
`run-1788702445-108011326-3183809`, including default-native debug/release. The new
default benchmark build passes format, 47 tests and strict clippy. Nine analysis
tests reject altered counts/configuration/rounds, bad counter coverage, overlap,
failed outcomes and diagnostic contamination. No fresh canonical run is claimed
for these documentation/evidence-only changes.

Source SHA-256:
`e344a60e6098206922dc615bad1bbbbfbff7c32badedc164d5b67f862f31a1b4`.
Executable SHA-256:
`ebf3684d18be022b60470546ada4b6b5297b5d73af8781cf6947506857d27193`.
The [durable bundle](evidence/quiet-checkpoint-e344a60e.tar.gz) contains all attempts,
commands, plans, raw outputs, counters, guards, late diagnostic, analysis scripts,
negative-control output, full source bytes and the preceding qualification bridge.
All 2,406 internal hashes verify, and its source bytes reproduce the source digest.
Archive SHA-256:
`e9f90434c9f5b3591cd6de93522058e8a3f3043801651cb6a16590e2206a59d3`.

## Next, separately attributable work

Keep the engine and direct mutex ownership frozen. The exact-owner controls do
not yet measure production owner-state frequencies, multi-waiter age or useful
handoffs versus intervening carrier work. Do not infer another ownership rewrite.
Capacity-independent observation remains open after the rejected zero/32-probe
experiments; no larger polling budget follows. The incremental-readiness candidate
remains held until protected throughput/cycle/idle/tail non-regression is established.
The completed channel attribution does not authorize another layout permutation.

Classify the park comparison failure before calling the full counter-free May
panel qualified. Separately retain the unclassified inbox-refill finding and
historical cancellation-history timing excursion as release blockers. Current-
source ARM64 execution, supported sanitizer integration, large mixed lifetimes/
simultaneous populations, footprint and offered-load tail/CPU qualification remain
open. There is no across-the-board May win, release completion or HTTP claim.
