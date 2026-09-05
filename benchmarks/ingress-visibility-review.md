# Idle ingress: progress before notification

Baseline: `62378f4b065be66852a48d3a17799ceec20b6b29` on
`perf/scheduler-hot-path`. This is a separate progress repair discovered during
capacity/idle attribution. **Capacity scans remain enabled.** No wake-word,
resource-ownership, ready-queue, placement or admission-window change is included.

## Production dependency and ordered reproduction

`Inbox::push` commits an accepted packet under its queue lock, publishes the
pending count, releases the lock, and then sends the empty-to-nonempty signal.
The packet can therefore be visible and drainable before the signal epoch changes.

The old idle predicate saw that packet and returned without sleeping. However,
the next carrier iteration only received remote work after an epoch change or
when its cached `remote_pending` flag was already true. The idle observation did
not set that flag. A paused notifier could leave a free carrier repeatedly
returning from idle without admitting the work it could already see.

The new regression runs the actual carrier and native fibers. An initial task
holds its owner after the first epoch has been handled. A second producer then
pauses **after queue unlock, before notification**. Releasing the first task
leaves one visible packet with the same epoch. The old source produced:

```text
task observation = Timeout
pending packets  = 1
epoch unchanged  = true
observed mounts  = 1
```

The repaired source executes the second task on the same owner before releasing
the producer: pending packets become zero, the epoch remains unchanged, and the
observed mount count is two. RAII releases both test gates and requests stop before
scoped joins, including observer failure. Assertions occur after cleanup. Five
seconds is a failure bound, not the ordering premise.

A second real-kernel test fails the old cached-flag behavior directly. A third
checks that a wake-only idle return does not claim remote starts. This is not the
separate wait-claim publication problem: no already-parked task is being selected,
and no task/resource generation is changed here.

## Repair and preserved contracts

Both idle checks use one helper that remembers visible remote ingress in the
existing carrier-owned flag. The next ordinary drive receives it without needing
a later signal. When no ingress exists, the helper performs the existing local
and remote wake predicates without writing the flag.

This carries an existing Acquire observation into owner-local state; it adds no
atomic operation, queue scan, allocation or clock read. Notification ordering,
stop/abort precedence in the drive loop, bounded ready-window admission and
mandatory task checkpoints remain unchanged. Started stacks do not migrate.
No atomic protocol is weakened or replaced. The new producer hook is test-only.

The repair does not make a queue lock nonblocking, guarantee wall-clock dispatch,
or fix selected-waiter publication tails. In particular, the probe pauses only
after releasing the queue lock; it is not evidence of progress while a producer
holds that lock.

## Qualification

Final source SHA-256:
`aa2fa66ebf6efafecaf9c6d8f393ece8916806c38bb24b9796f0fb799abcfa08`.

- All 11 canonical gates passed under receipt
  `/root/.cache/zcheck/run-1788639500-207536761-2392952/receipt.json`, including
  535 all-feature runtime tests (one ignored).
- The separate default-native workspace passed 515 runtime tests (one ignored),
  70 stack tests and the remaining lab/model/reexport suites.
- All three final ingress regressions passed with optimized native fibers and
  in 20 serial one-CPU debug processes (60 selected test executions).
- Two 60-second optimized native mixed soaks completed 367,770 and 337,479 task
  lifetimes on one and four carriers: 705,249 total. All completed, park/wake
  totals matched, and the existing service/shutdown assertions passed.
- The standalone benchmark's 36 all-feature tests and all-target/all-feature
  warning-denying Clippy check passed.

The source-keyed [evidence bundle](evidence/ingress-visibility-aa2fa66e.tar.gz)
preserves the receipt and raw logs, source patches, commands, host observations,
analysis and binary hashes. Its digest is recorded in [the index](evidence/README.md).

The early optimized run used temporary diagnostic printing; its log is retained
separately. Optimized tests were rerun after the final assertion-only cleanup.
The architecture refresh changes source inventory only, with zero new grants,
revocations, dependencies or debt. These are Linux results, not new ARM64 evidence.

## Performance observations and acceptance limits

The default binaries were measured in three independent, balanced process pairs
per reported case. Initial yield/lifecycle/park controls overlapped another
project's build/test job; that driver was paused and those cases repeated under
new filenames. Synchronization cases below pin the four carriers using the existing
benchmark option; single-carrier mechanism runs use CPU 7. Placement policy itself
is unchanged. Cycles cover the whole process, including warm-up and shutdown;
ns/op is the median whole-round duration divided by useful operation count.

| Case: carriers / tasks | Baseline ns/op | Repair ns/op | Process cycles change |
| --- | ---: | ---: | ---: |
| Yield, 1 / 64 | 46.14 | 45.86 | -1.15% |
| Yield, 4 / 64 | 15.09 | 15.75 | +6.24% |
| Lifecycle, 4 / 1,000 | 428.77 | 426.24 | -1.03% |
| Lifecycle, 4 / 10,000 | 367.33 | 342.11 | -3.51% |
| Park, 1 / 64 | 237.53 | 238.59 | +0.52% |
| Park, 4 / 64, pinned | 93.63 | 93.18 | -2.74% |
| Contended mutex, 4 / 64, pinned | 403.55 | 438.49 | -1.84% |
| Uncontended mutex, 1 / 1 | 27.74 | 27.74 | +0.04% |
| Historical channel, 4 / 64, pinned | 151.07 | 147.97 | -5.15% |
| Timestamped wake throughput, 4 / 64, pinned | 148.68 | 149.49 | -1.19% |
| Park, 4 / 64, capacity 65,536, pinned | 1,093.34 | 1,228.34 | +8.54% |

These observations **do not qualify a general performance win or zero regression**.
The narrow single-carrier yield and uncontended-mutex instruction totals are
effectively unchanged. The larger lifecycle case is encouraging, but multi-carrier
timing/cycles are mixed; later host snapshots also caught another Python job during
refreshed park runs. External work was not stopped or repinned. Host activity does
not by itself explain every difference, including the capacity and mutex results.

Timestamped wake p50 stayed around 230-240 ns. Across the three processes, baseline
p99.9 was 21.0-23.7 us and repair p99.9 was 21.0-25.0 us. A repair process reached
1.44 ms at p99.99, and both binaries retained tens-of-milliseconds maxima. These
tails are not accepted as improved or attributed solely to the patch or OS.
The remaining quiet-host regression/tail gate is explicitly open.

This slice is retained for its ordered, native progress proof, not promoted as a
qualified optimization. No new May comparison was run. The larger capacity/idle
optimization still requires uninstrumented performance acceptance.

## Connection to the capacity experiment

The separately shelved observer-only experiment removed carrier-side depth scans
without changing idle policy. Instrumented lifecycle runs then showed near-singleton
batches, many more poll probes and millions of empty idle reentries. That evidence
motivated the paused-producer regression; it does not prove the cached-flag issue
accounts for all of the lifecycle cycle penalty.

The [observer experiment](capacity-pacing-review.md) is preserved at stash
`0b76c091e851474dbda06788ee1a10890c0f61fe`, with immutable binaries and raw
source-keyed output archived in [the evidence index](evidence/README.md). It is not
an accepted runtime change. Scan removal must be remeasured on the repaired baseline, then
meet the lifecycle, spare-capacity, tail and fairness gates before retention.

The six separately recorded loaded-suite findings in
[wait-qualification-review.md](wait-qualification-review.md) remain open. This
reproduction does not establish the cause of the earlier continuous-refill timeout.
HTTP remains outside this work.
