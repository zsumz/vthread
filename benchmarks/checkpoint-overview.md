# Core performance overview: 2026-09-05

This refresh measures the requested borrowed-occupancy work-in-progress checkpoint
on `perf/scheduler-hot-path`, based on `c7beec5`. Its source and executable identities
are in the [occupancy review](borrowed-occupancy-review.md). The tested executable
contains both engines: default-native vthread and May 0.3.51. No new runtime change
was made for these comparisons. They are a snapshot, not an optimization acceptance
or a declaration that the overall May goal is complete.

## Protocol and interpretation

All 40 invocations ran serially, in fresh processes, without overlapping builds,
tests, soaks, profiling or disassembly. Each workload has two invocations per engine
with reversed order. V/M/M/V is used for yield, 10,000-task lifecycle, mutex,
historical channel and wake-tail; M/V/V/M is used for the other rows.

Four-carrier workloads use guest CPUs 0-3 and 64 tasks unless a row says otherwise.
The uncontended mutex uses one carrier, one task and guest CPU 7; TCP uses four
carriers and eight task-owned connections to a native loopback echo peer.
There is one warm-up per process. Throughput uses 100,000 operations per task and
nine measured rounds. Uncontended mutex uses ten million acquisitions, wake-tail
10,000 wakes/task, TCP 1,000 round trips/task, and lifecycle 101 measured rounds.
Both engines are configured with 64 KiB stacks. Vthread admission capacity matches
the task count. The 10,000-task lifecycle rounds retain wide tails in both engines.

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
AMD EPYC 9555P. No RUSTFLAGS. Physical-host placement, frequency and competing host
load are unknown. Results and large ratios are specific to this guest/protocol,
not portable hardware claims. No cycle counters were collected in this refresh.

May keeps its default worker pinning and can migrate coroutines. Vthread is not
individually pinned and never migrates a started task. Warm-up placement reports
32 cross-carrier vthread pairs versus 32 finally co-located May pairs in the park
and capacity-gated SPSC controls. Those reports describe warm-up, not every timed
handoff, and cannot quantify migration's contribution. Equal worker counts are
not identical execution topology. The earlier [pinning control](mutex-placement-review.md)
did not explain away the mutex gap.

## Throughput and lifecycle

Cells show the two invocation medians in execution order for that engine. Units
are ns/operation, except lifecycle, which is ns/completed-and-drained task. These
are whole-round elapsed times divided by operation counts, not latency medians.
Ratios compare the arithmetic means of the two invocation medians; they are
descriptive, not confidence intervals.

| Workload | Vthread | May | Direction in this refresh |
| --- | ---: | ---: | --- |
| Yield, 4 carriers / 64 tasks | 15.85 / 13.60 | 21.77 / 16.98 | Vthread ~1.3x faster |
| Lifecycle, 4 / 1,000 | 390.06 / 400.62 | 2,865.88 / 2,873.29 | Vthread ~7.3x faster |
| Lifecycle, 4 / 10,000 | 339.03 / 335.74 | 5,716.19 / 5,421.83 | Vthread ~16.5x faster |
| Park handoff, 4 / 64 | 92.98 / 94.16 | 62.82 / 60.71 | May ~1.5x faster |
| Contended mutex, 4 / 64 | 358.82 / 364.84 | 159.56 / 159.80 | May ~2.3x faster |
| Uncontended mutex, 1 / 1 | 27.78 / 27.76 | 40.02 / 40.16 | Vthread ~1.4x faster |
| Historical channel, 4 / 64 | 139.84 / 144.23 | 76.24 / 71.22 | May ~1.9x faster; unequal backpressure contracts |
| Capacity-one SPSC control, 4 / 64 | 146.34 / 144.69 | 25.07 / 24.83 | May ~5.8x faster; see qualification below |
| Timestamped wake throughput, 4 / 64 | 136.91 / 145.85 | 75.98 / 91.04 | May ~1.7x faster |
| TCP whole-round throughput, 4 / 8 | 13,600.90 / 13,929.29 | 12,094.33 / 11,723.41 | May ~1.16x faster; includes peer lifecycle |

The historical channel is bounded-one vthread versus May's unbounded MPSC.
The capacity-one control uses vthread's general bounded channel against May SPSC
with a May coroutine-aware semaphore enforcing capacity. May has no native bounded
channel in this harness version. This matches capacity in this SPSC workload, not
the full API, cancellation, fairness, migration or allocation contracts. Strict
ping-pong has at most one outstanding message, so this does not exercise sustained
buffer saturation. It is not an MPSC/MPMC or full channel backpressure matrix.

## Individual wake latency and observed fairness

Each invocation records 5,760,000 atomic-timestamp wake-to-resume observations.
Columns below retain the actual V/M/M/V execution order. All values are ns.

| Statistic | V1 | M1 | M2 | V2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 210 | 511 | 671 | 210 |
| p95 | 1,242 | 1,883 | 1,763 | 731 |
| p99 | 65,759 | 2,033 | 1,953 | 82,635 |
| p99.9 | 113,450 | 3,425 | 3,755 | 143,847 |
| p99.99 | 145,289 | 7,832 | 7,481 | 151,829 |
| Maximum | 49,909,322 | 49,434,087 | 45,839,262 | 49,521,268 |
| Worst-pair p99.9 | 119,791 | 4,216 | 4,427 | 145,839 |
| Task-median range | 191-230 | 110-901 | 241-1,312 | 180-280 |

Vthread's median is 2.4-3.2x faster, and task medians are more tightly grouped in
these samples. May's p99 through p99.99 are substantially better. Roughly 50 ms
maximum outliers affect both engines, but that does not explain away the much
larger vthread p99.9. Median leadership is not tail-latency or fairness leadership.
There is no background-load, per-acquisition mutex-fairness, or topology matrix here.

## Readiness-driven TCP latency

Each invocation records 72,000 one-byte write/read round trips. Values below are
ns in M/V/V/M order. Socket peer setup and shutdown are outside each individual
round-trip timestamp, unlike the whole-round throughput measurement above.

| Statistic | M1 | V1 | V2 | M2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 20,661 | 18,227 | 18,938 | 20,641 |
| p99 | 64,867 | 171,077 | 142,455 | 83,255 |
| p99.9 | 4,474,569 | 10,967,730 | 15,878,155 | 2,969,343 |
| p99.99 | 50,242,845 | 45,076,545 | 85,555,933 | 45,569,717 |
| Maximum | 93,326,195 | 54,967,711 | 101,573,028 | 57,457,842 |

Vthread's median is about 1.1x faster, while whole-round throughput and p99/p99.9
favor May. This is a loopback readiness benchmark, not HTTP or an application-level
win. The HTTP arena remains a separate project.

## Checkpoint scope and next decisions

The native-stack foundation, compact task storage, direct-handoff mutex and
cancellation-safe selection-under-lock repair remain in place. The latest local
change replaces borrowed tracking rediscovery with the slab's existing O(1) count;
its isolated probe uses about 98% fewer cycles. It is pushed as requested without
pretending that gain improves every scheduler dispatch.

All 11 canonical gates passed for the unchanged runtime source measured here, as
did the final default-native workspace suite (496 runtime tests), 33 benchmark
tests and approximately 845,000 mixed/borrowed stress lifetimes. The occupancy
review records the earlier load-associated stall-test failure and quiet pass,
unresolved ~1.7% uncontended-mutex cycle increase, noisy channel regression
observations, source hashes and exact qualification limits. No complete
ten-million-lifetime, sanitizer or cross-architecture claim is made.

The [two ownership experiments](mutex-ownership-review.md) and earlier runtime
mailbox integration were rejected; their failed performance gates must not be
counted as shipped wins. The mailbox models remain useful test-only groundwork.
The next core priorities are channel handoff, contended ownership transfer and
general-wake tail fairness, while resolving the checkpoint's regression questions.
No change to post-mount affinity or cancellation semantics is authorized by these
numbers. The four-case May goal is still unmet.

## Evidence and reproduction

Raw files are `/tmp/vthread-borrowed-count-DTX6B7/overview-CASE-N-ENGINE.log`.
All 40 invocations exited successfully; every throughput sample and placement/
latency/fairness report is preserved locally. The files are not a durable release
evidence bundle. The benchmark SHA-256 is
`636d46dc41eede59a49672917d3189ff1bccdb501043e9ac2633b2e537f49458`.
See [the harness](README.md) for operation definitions and supported controls.

```sh
timeout 120s taskset -c 0-3 BINARY ENGINE yield 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY ENGINE spawn 4 1000 101
timeout 120s taskset -c 0-3 BINARY ENGINE spawn 4 10000 101
timeout 120s taskset -c 0-3 BINARY ENGINE park 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY ENGINE mutex 100000 4 64 9
timeout 120s taskset -c 7 BINARY ENGINE mutex-uncontended 10000000 1 1 9
timeout 120s taskset -c 0-3 BINARY ENGINE channel 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY ENGINE channel-bounded-spsc 100000 1 4 64 9
timeout 120s taskset -c 0-3 BINARY ENGINE wake-tail 10000 4 64 9
timeout 120s taskset -c 0-3 BINARY ENGINE tcp 1000 4 8 9
```
