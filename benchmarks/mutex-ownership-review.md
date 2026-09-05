# Mutex ownership experiments: 2026-09-05

Both runtime candidates below were rejected and removed. The retained baseline is
`c7beec5`, including its cancellation-safe selection-under-lock repair. Neither
experiment establishes a new May win. The existing stash was left untouched.

## Serialized ownership and waiter accounting

The first candidate placed ownership and outstanding-waiter accounting under the
existing queue lock. Publication and extraction used exclusive mutable access to
the ownership slot; a separate atomic mirror preserved O(1) `waiting()` snapshots.
It removed several locked operations but required the resumed recipient to acquire
the queue lock. Capacity and selected-but-not-retired tickets remained bounded.

The targeted native mutex tests, exclusive-core tests, and extended cancellation
composition model passed. This was not complete canonical qualification.

Four-carrier/64-task mutex A/B/B/A medians were
360.12 / 356.23 / 355.98 / 359.91 ns/op. Single-carrier forced-contention counters
fell by about 3.9% in cycles but added approximately eleven instructions per
acquisition. Four-carrier cycle totals were
103,909,784,460 / 142,201,701,971 / 134,815,533,050 / 135,651,488,640.
The intended multi-carrier cycle reduction was not demonstrated. The extra queue
acquisition was not retained for the small elapsed-time difference.

Local binaries, patches, test output and raw measurements are preserved in
`/tmp/vthread-mutex-serialized-hK1ZtK`. The candidate executable SHA-256 is
`114166e0197c2700dd4871ea2823a7e441328b0f1f01d650bd2eefc3b5aebc01`.

## Cell-bound ownership publication

The second candidate bound a handoff slot to a cell's never-reused identity. A
linear ownership capability authorized a Release store instead of a publication
CAS. The recipient used an Acquire CAS that left an empty slot untouched. Foreign
capabilities were rejected. This added an immutable identity word to the slot,
but no allocation, queue lock or unsafe code. FIFO and checkpoints were unchanged.

An earlier unconditional-swap consumer failed the production-source Loom test.
A reduced atomic-only store/swap test also failed with fully SeqCst operations in
Loom 0.7.2. This does not establish a native runtime defect; the model failure was
not waived and that version was never timed. The conditional-take version passed
all four shared ownership tests under Loom, including competing consumers and
immediate republication. The queue/claim adapter separately passed competition
with direct cancellation, inherited cancellation, timeout and close. All 32 core
unit tests and 15 targeted native mutex tests passed. No exhaustive model cutoff
was used; exceeding the branch bound fails the test. Full canonical qualification
was not attempted after the performance gate failed.

Values below are serial A/B/B/A invocations, not concurrent engine runs:

| Measurement | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex, 4 carriers / 64 tasks, median ns/op | 375.08 | 367.21 | 363.40 | 359.82 |
| Same, whole-round maximum normalized ns/op | 397.70 | 390.92 | 376.84 | 394.19 |
| Forced contention, 1 carrier, cycles | 21,890,229,169 | 21,567,207,650 | 21,552,862,129 | 21,781,251,379 |
| Same, instructions | 55,792,779,308 | 55,741,234,850 | 55,740,775,738 | 55,792,154,747 |
| Mutex, 4 carriers, cycles | 191,693,182,030 | 136,359,185,420 | 138,957,736,244 | 112,648,183,276 |
| Same, instructions | 95,202,551,515 | 80,100,624,894 | 80,897,955,216 | 72,695,936,259 |

Single-carrier cycles improve about 1.3%, with two fewer instructions per
acquisition. Four-carrier ordering reverses between the two run orders. Instrumented
four-carrier processes recorded approximately 1.31 million / 644 thousand /
858 thousand / 417 thousand OS context switches; A1 and B2 included whole rounds
of 28.18 and 14.55 seconds. These observations are not a demonstrated multi-carrier
cycle win. No per-operation p99.9 or fairness qualification is claimed for either
rejected candidate. Whole-round maxima above are not latency percentiles.

Local evidence is in `/tmp/vthread-mutex-bound-slot-HM4zzS`, including the reduced
model reproducer, initial failures, final passing tests, source/environment
manifest, tracked patch, new-file archive and immutable executables. The baseline
SHA-256 is `56dc46a15b73a7647e444f9c548188de92ab914c9f02d3b976675cfc94c7eb2a`;
the measured `conditional` candidate is
`ecf66db66c610526ca4e2ffbc78b0cbf283766daf9b7426a8b679df1792ba62e`.
These temporary artifacts are not a durable release-evidence bundle.

## Reproduction and scope

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
AMD EPYC 9555P. Four-carrier process mask: guest CPUs 0-3, without individual carrier
pinning. Single-carrier mask: CPU 7. Physical-host placement and frequency policy
are unknown. No build, test, soak, disassembly or other benchmark overlapped timing.
Throughput uses one warm-up and nine measured rounds, 100,000 operations per task.
Counters use one warm-up and three measured rounds: 25.6 million acquisitions,
including the forced yield in the single-carrier case. They include process setup.

```sh
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s perf stat -x, -e cycles,instructions taskset -c 7 BINARY vthread mutex 100000 1 64 3
timeout 120s perf stat -x, -e cycles,instructions taskset -c 0-3 BINARY vthread mutex 100000 4 64 3
```

The bound-slot counter runs additionally record `task-clock`, `context-switches`
and `cpu-migrations`. Keep that instrumentation consistent within a comparison.
