# Borrowed occupancy checkpoint: 2026-09-05

This is the requested work-in-progress checkpoint on `perf/scheduler-hot-path`,
not acceptance under the review's complete performance rule. Correctness gates
pass, and the isolated maintenance operation is much cheaper. The small
uncontended-mutex cycle increase and noisy channel regression observations remain
open. No general scheduler speedup or new May win is attributed to this patch.

## Narrow change and reproduced regression

The scheduler already gates borrowed revocation sweeps on a changed epoch. It did
not scan every queue on every dispatch, as the original review suggested.
`refresh_borrowed()` nevertheless searched in-flight, ready and parked tasks to
rediscover a boolean. Normal completion also left that boolean set after the last
borrowed task was reclaimed, permitting an unnecessary sweep at the next epoch.

The patch reads the existing borrowed slab occupancy in O(1) and refreshes the
cached boolean when a borrowed in-flight slot is removed. It introduces no new
counter, allocation, atomic protocol, unsafe code, task layout or ready policy.
Actual revocation of live borrowed tasks still scans at a changed epoch; this is
not elimination of all linear scheduler maintenance.

The new deterministic regression completes a borrowed child, admits 64 unrelated
owned tasks, and publishes scope exit. It failed on `c7beec5` because the cached
tracking flag remained set. The checkpoint clears that flag immediately and
inspects zero unrelated tasks. A separate test checks occupancy through removal,
duplicate removal, intermediate empty records and slab-slot reuse.

## Isolated cycle reduction

The ignored release-mode probe constructs 64 real owned ready records and calls
`refresh_borrowed()` ten million times on guest CPU 7. A is `c7beec5` with the probe;
B is this checkpoint. Each column is a fresh process in serial A/B/B/A order.
Counters include setup and teardown, not just the timed loop.

| Measurement | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 2,809,671,232 | 49,497,194 | 53,539,185 | 2,823,782,612 |
| Instructions | 12,296,335,179 | 135,108,877 | 135,144,655 | 12,296,073,265 |
| Timed ns/refresh | 119.22 | 4.37 | 0.56 | 125.88 |

Total cycles fell approximately 98.2% and instructions 98.9% in this isolated
probe. B1's wall-time pause despite similar cycle totals illustrates guest timing
variation. These are not end-to-end local-scope results, per-yield savings, or a
stable sub-nanosecond latency claim.

## Cross-workload observations and unresolved costs

The same A/B/B/A comparison uses immutable benchmark executables. Values are
invocation-median ns/operation, or ns/completed-and-drained task for lifecycle.
Four-carrier cases use guest CPUs 0-3, without individual carrier pinning.

| Workload | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex, 4 carriers / 64 tasks | 370.02 | 365.88 | 352.43 | 366.93 |
| Park, 4 / 64 | 91.74 | 90.74 | 92.33 | 91.43 |
| Historical channel, 4 / 64 | 141.54 | 149.38 | 141.63 | 143.56 |
| Yield, 4 / 64 | 13.42 | 13.50 | 13.59 | 13.57 |
| Uncontended mutex, 1 / 1 | 22.76 | 27.63 | 27.63 | 22.76 |
| Lifecycle, 4 / 1,000 | 416.15 | 418.98 | 387.63 | 406.56 |
| Lifecycle, 4 / 10,000 | 360.72 | 362.66 | 353.94 | 337.49 |

Throughput uses 100,000 operations/task, one warm-up and nine measured rounds.
Uncontended mutex uses ten million acquisitions; lifecycle uses 101 measured
rounds. Whole-round maxima are not individual-operation latency percentiles.

The uncontended rounds are bimodal, near 225 and 277 ms. Follow-up A/B/B/A counters
use one warm-up plus three measured rounds, or 40 million acquisitions/process:

| Measurement | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 2,240,374,822 | 2,279,008,710 | 2,275,670,838 | 2,238,591,354 |
| Instructions | 5,165,604,933 | 5,165,987,241 | 5,166,000,532 | 5,165,447,120 |
| Median ns/acquisition | 27.60 | 27.87 | 22.75 | 27.58 |

The apparent 21% wall-time regression does not persist in those medians. However,
the checkpoint uses about 1.7% more cycles, approximately 0.95 extra cycles per
acquisition, with nearly identical instructions. That smaller cost is unresolved;
it must not be reported as zero regression.

A 501-round, 10,000-task lifecycle follow-up gives A/B/B/A medians of
338.15 / 337.62 / 337.66 / 342.25 ns/task and total cycles of
23,376,202,668 / 23,216,826,029 / 23,388,712,118 / 24,293,746,056.
It does not show a lifecycle regression.

A separate channel B/A/A/B repeat gives 152.40 / 138.83 / 150.92 / 146.52 ns/op,
with whole-round maxima normalized per operation of
173.31 / 148.37 / 164.72 / 154.75. The ordering reverses between pairs, but averaging
the two invocation medians still makes B about 3.2% slower (about 2.1% in the first
comparison). No neutral-regression verdict is claimed while this remains unresolved.
The historical channel compares vthread capacity one with May's unbounded MPSC;
it is not the separate capacity-gated SPSC suite.

## Wake tails

Each invocation records 5,760,000 timestamped wakes, four carriers / 64 tasks.

| Statistic, ns | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 210 | 210 | 201 | 201 |
| p99.9 | 114,623 | 113,822 | 113,972 | 142,444 |
| p99.99 | 145,269 | 146,220 | 143,487 | 151,458 |
| Worst-pair p99.9 | 128,103 | 115,535 | 136,306 | 144,949 |
| Maximum | 49,031,791 | 49,954,313 | 49,123,070 | 49,848,065 |

Task-median ranges are 200-220 / 191-220 / 180-240 / 181-240 ns. This sample has
no consistent p99.9 regression; roughly 50 ms outliers remain. It does not qualify
loaded tails, mutex-acquisition fairness or the full topology matrix.

## Correctness qualification and limits

All 11 canonical gates passed under receipt
`/root/.cache/zcheck/run-1788614758-907960207-2120048/receipt.json`.
The final default-native workspace rerun passed: 496 runtime tests (one manual
probe ignored), 66 stack tests, 27 synchronization-core unit tests, 10 handoff/word
model tests, 15 mailbox-model tests, 19 lab tests and the alias-consumer test.
The standalone benchmark suite passed all 33 all-feature tests and clippy.
The architecture lock updates analyzed inventory only: no grants, dependencies,
policy permissions or ratchets changed.

An earlier native run overlapping other qualification failed
`unrelated_supervisor_activity_cannot_hide_a_stalled_root_scope`. Its watchdog
uses a fixed 200 ms park before waking the target; a scheduling assumption is
suspected, but the exact returned result was not logged and the cause is not
conclusively established. The quiet rerun passed. Neither the test nor runtime
stall detection was changed in this slice; the failure remains follow-up work.

| Native mixed soak | Completed lifetimes | Checked mutex updates | Matching parks/wakes |
| --- | ---: | ---: | ---: |
| 60 seconds, 1 carrier / 64 workers | 363,630 | 2,698,240 | 3,852,369 |
| 60 seconds, 4 carriers / 64 workers | 350,520 | 2,600,960 | 3,605,213 |

Those soaks check channels, mutexes, TCP, timers, cancellation, native work, stack
reuse and shutdown, but do not create borrowed local scopes. They overlapped
qualification and are correctness evidence, not throughput measurements.
A separate native borrowed-scope stress uses 64 joined parents, 1,024 borrowed
children per parent, and seeded 0-7 yields per child (`0x93a458e7d0216bcf`).
Both one- and four-carrier runs verify owner affinity, borrowed values, every join,
65,600 completed tasks and empty final admission/wake state. Combined evidence is
845,350 task lifetimes. The borrowed stress does not inject cancellation or panic;
those paths have unit coverage, not this long-run coverage. This is not ten million
lifetimes, a sanitizer run, or full cross-architecture qualification.

## Reproduction and source identity

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
AMD EPYC 9555P. Physical-host placement and frequency policy are unknown. No builds,
tests, soaks, disassembly or other benchmarks overlapped timed invocations.

```sh
cargo test --release --locked -p vthread --lib borrowed_tracking_refresh_probe -- --ignored
timeout 120s taskset -c 0-3 BINARY vthread channel 100000 4 64 9
timeout 120s perf stat -x, -e cycles,instructions taskset -c 7 BINARY vthread mutex-uncontended 10000000 1 1 3
timeout 120s perf stat -x, -e cycles,instructions taskset -c 0-3 BINARY vthread spawn 4 10000 501
```

For the probe, use the full test path
`kernel::kernel_revoked::kernel_revoked_test::borrowed_tracking_refresh_probe`
with `--exact`, pin to CPU 7, and set `VTHREAD_BORROWED_PROBE_ITERATIONS=10000000`.
Build before timing. Run immutable A/B/B/A binaries serially.

Local raw logs, regression failure, manifests, stress source and executables are
in `/tmp/vthread-borrowed-count-DTX6B7`; they are not a durable release bundle.
Final runtime/tooling source SHA-256, computed by `scripts/evidence.py`:
`db2d93ff0ecb156f1fae41b1a1cae66764c4ac478f6ff5a9cf90146b6ecd8189`.
Baseline benchmark SHA-256:
`56dc46a15b73a7647e444f9c548188de92ab914c9f02d3b976675cfc94c7eb2a`.
Checkpoint benchmark SHA-256:
`636d46dc41eede59a49672917d3189ff1bccdb501043e9ac2633b2e537f49458`.
Final probe SHA-256:
`3a7e01c729df3860087a9b95e433f162c50f7b2bf7aedd2ff230105fa3c3be47`.
Borrowed-stress source SHA-256:
`8001a679dfc1b59ea3e6d14f3c6402173b5ae1c078916a7a872a49b89b6342b3`.
