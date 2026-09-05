# Mutex handoff cancellation repair: 2026-09-05

This is a correctness repair to `c884534`, not a claimed performance optimization.
The mutex remains bounded and FIFO, retains logical ownership during handoff, and
preserves both cancellation checkpoints and carrier-affine guards.

## Reproduced failure

The former unlock sequence removed a waiter and published its ownership capability
under the queue lock, then offered its resource **after** releasing that lock.
Cancellation could finish the removed ticket in that window. Cleanup found neither
a queue entry nor a resource, and returned without recovering ownership. The later
offer succeeded against the now-idle reusable wait cell, but no ticket remained to
consume it: `waiting() == 0` while `try_lock()` permanently returned `WouldBlock`.

The deterministic native-stack regression stops a native owner exactly after queue
removal, cancels and drives the parked vthread, then allows publication to continue.
It failed on the old ordering with `cancelled recipient orphaned ownership` and
passes with the repair. Test gates are compiled out of non-test builds.

## Repair and invariant

Unlock now reserves the wait generation while it still holds the same queue lock
used by cancellation cleanup. A ticket is either still queued or already has a
selected resource when cleanup observes its removal. Wake routing remains outside
the queue lock. The deferred publication guard owns the queue's existing `WaitCell`;
it adds no allocation or reference-count clone. Dropping it during unwind publishes
the selected wake so neither the generation nor its ownership is stranded.

The resource consumer also now respects write-exclusive Binding and Claim phases.
Previously it could CAS away a resource before a publisher's final Release store
restored the old word. A separate step-level regression failed on that invariant
before the repair. This test establishes the word-level defect; the mutex test above
establishes the reachable ownership leak in the actual runtime.

No atomic ordering is weakened. The ownership capability and slot, bounded waiter
accounting, queue order, scope ownership, routing generations, and public guard's
non-Send contract are unchanged. No scheduler rewrite or new dependency is involved.

## Qualification scope

The new standalone Loom harness includes the production `WaitWord` source and
resource transition functions. Its adapter preserves the runtime's queue-lock
boundary, AcqRel/Acquire word CAS, Release Claim publication, and the ownership
slot's Release/Acquire transfer. It explores one owner, one recipient, one queue
entry and one generation without a permutation, preemption, or duration cutoff.
The branch limit fails the test rather than silently truncating exploration.

The old ordering is a negative control that must reproduce leaked ownership.
The repaired ordering passes races against direct and inherited cancellation,
timeout and close; resource cleanup also races Claim publication. The harness
counts wake publications rather than executing the carrier scheduler. It is not a
model of arbitrary queue lengths, native stacks, the full routing implementation,
or all shutdown interleavings. Native tests separately cover FIFO successors,
selected cancellation, idle grants, queue capacity, actual routing and unwind.

The added native regressions verify cancellation in the dequeue/publication window,
idle-ticket cleanup before deferred publication, and unwind after queue removal.
Wait-resource tests verify deferred routing, losing selectors, single publication
on unwind, and no late mutation after an idle grant is consumed and recycled.

The default-native workspace suite passes: 494 runtime tests, 66 stack-engine tests,
27 synchronization-core unit tests, 10 handoff-model/word tests, 15 mailbox-model
tests, 19 lab tests and the external alias-consumer test. The handoff harness's ten
tests comprise six Loom tests and four production word tests, not ten Loom models.
All 11 canonical gates pass under receipt
`/root/.cache/zcheck/run-1788600079-568552406-2105663/receipt.json`. This and the
default-native rerun use the final, simplified publication guard. The first canonical
attempt stopped at a model-harness style warning; the final run includes that fix.
The standalone benchmark suite also passes all 33 all-feature tests and clippy.
The architecture lock updates analyzed inventory only, with no new grants,
dependencies, gate permissions, or ratchets.

The native release mixed soak completed the following 60-second runs:

| Carriers | Workers per batch | Batches | Completed lifetimes | Checked mutex updates | Parks and wakes |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4 | 64 | 4,394 | 303,186 | 2,249,728 | 3,117,288 each |
| 1 | 64 | 5,298 | 365,562 | 2,712,576 | 3,872,836 each |

Each verifies payloads, affinity, reclamation, bounded stack reuse and drained
readiness/native services at shutdown. These soaks overlapped canonical
qualification: the counts are correctness evidence, not throughput measurements.
They are not the review's full ten-million-lifetime, sanitizer or architecture matrix.

Local before/after logs, model output, and the immutable pre-repair benchmark are in
`/tmp/vthread-mutex-cancel-gap-dNjMTy`. The baseline executable SHA-256 is
`8bfe3ffd6d9c81ecc0a2d72269bd8f4c75e3cf8a670a0cc3298bc691adcf29e6`.
The initial repair executable SHA-256 is
`8119c391989c1de76abb7b2cb58ea04fc0bd49838b69f4422c6c53a5365c03b8`.
The final `repair-drop` executable uses Drop as the single publication path, also
for explicit publication, and has SHA-256
`56dc46a15b73a7647e444f9c548188de92ab914c9f02d3b976675cfc94c7eb2a`.
These local artifacts are not an immutable release-evidence bundle.

```sh
cargo test --locked -p vthread --lib mutex
cargo test --locked -p vthread --lib wait_resource_test
cargo test --locked -p vthread-sync-core --test mutex_handoff_model
cargo test --locked --workspace --all-targets
CARGO_INCREMENTAL=0 zcheck run check
```

## Serial performance regression observations

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
an AMD EPYC 9555P. Four-carrier processes use guest CPUs 0-3 without the optional
carrier-pinning flag. Single-carrier controls use guest CPU 7. Physical-host placement
and frequency policy are unknown. No builds, tests, soaks, other benchmarks or
disassembly overlapped any timed invocation. Source/environment and topology
manifests accompany the local logs.

The initial repair's four-carrier mutex A/B/B/A medians were
356.45 / 362.91 / 376.47 / 366.51 ns/op, with whole-round maxima normalized per
operation of 370.52 / 422.15 / 387.43 / 379.37. That was not accepted as neutral.
Inspection found redundant normal-path publication-guard cleanup, which was
simplified to use the same Drop path for both explicit publication and unwind.

The following table uses that final guard. A is the pre-repair baseline; C is the
final repair. Each row is a separate serial A/C/C/A comparison, not concurrent work.
Values are median ns/operation; lifecycle rows use ns/completed-and-drained task.

| Workload | A1 | C1 | C2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex, 4 carriers / 64 tasks | 356.60 | 361.02 | 352.86 | 360.35 |
| Park, 4 / 64 | 91.28 | 91.09 | 91.07 | 90.37 |
| Channel, 4 / 64 | 139.33 | 143.47 | 143.92 | 145.58 |
| Yield, 4 / 64 | 13.87 | 14.06 | 13.31 | 13.33 |
| Uncontended mutex, 1 / 1 | 22.73 | 22.71 | 22.72 | 22.72 |
| Lifecycle, 4 / 1,000 | 405.29 | 399.32 | 399.50 | 406.06 |
| Lifecycle, 4 / 10,000 | 334.74 | 331.23 | 332.79 | 337.91 |

Throughput invocations use one warm-up and nine measured rounds, 100,000 operations
per task; uncontended mutex uses 10,000,000 acquisitions. Lifecycle invocations use
one warm-up and 101 measured rounds. The final mutex whole-round maxima, normalized
per operation, were 369.78 / 389.20 / 366.09 / 368.81 ns. These whole-round values
are not individual-operation p99 or p99.9. Lifecycle has wide round tails in both
binaries, including roughly 53 ms rounds at 10,000 tasks. Neither those tails nor
the small median differences establish an optimization win.

### Measured cost remains

Forced-contention single-carrier counters use 64 tasks, 100,000 acquisitions per
task, one warm-up and three measured rounds: 25.6 million acquisitions per invocation.
The final A/C/C/A cycle totals are
21,927,659,037 / 22,161,730,775 / 22,087,828,656 / 21,867,685,064.
Instruction totals are
55,536,752,190 / 55,795,545,983 / 55,792,974,451 / 55,535,722,148.
The final repair costs about **1.04% more cycles** and **10.1 additional instructions
per acquisition** in this control, including the unchanged forced yield. Counters
include setup and warm-up. The guard simplification did not establish a cycle win.

This cost is retained to close a reproduced permanent-lock defect. The four-carrier
medians are approximately flat; neither outcome is a claim of beating May, nor a
reason to stop optimizing handoff bookkeeping.

### Wake tails and fairness observations

Each invocation records 5,760,000 timestamped wakes (10,000 per task per measured
round, four carriers / 64 tasks). A/C/C/A observations are:

| Statistic | A1 | C1 | C2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median, ns | 200 | 201 | 200 | 201 |
| p99.9, ns | 139,010 | 140,582 | 114,262 | 140,933 |
| p99.99, ns | 147,342 | 149,155 | 142,375 | 148,283 |
| Worst-pair p99.9, ns | 140,602 | 141,964 | 138,409 | 143,005 |
| Maximum, ns | 49,561,063 | 49,929,919 | 53,447,340 | 49,096,955 |

Task-median ranges were 171-260 / 190-241 / 190-230 / 170-260 ns. No consistent
p99.9 or median-spread regression appears in this sample, but the final repair has
the worst observed maximum. The guest's large tails remain unresolved; this is not
a loaded-tail or mutex-acquisition-fairness qualification.

Reproduce each binary separately, alternating invocation order:

```sh
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s perf stat -x, -e cycles,instructions taskset -c 7 BINARY vthread mutex 100000 1 64 3
timeout 120s taskset -c 0-3 BINARY vthread wake-tail 10000 4 64 9
```
