# Scheduler activity: final-only diagnostic counters

Baseline: `8b53ae98ad0f8ef85b5494f334dff8a498e23ee9` on
`perf/scheduler-hot-path`. This slice adds opt-in evidence for the capacity,
admission and idle-pacing investigation. It does not remove the pending-wake
scan or change admission, wake ordering, ready fairness, resource ownership,
polling limits or the native wait protocol. HTTP remains separate.

## Measurement boundary

The `scheduler-profiling` feature keeps cumulative counters in each owning
kernel. Updates use ordinary owner-local integers and a fixed eight-bin drain
histogram. There are no per-task timestamps, new counter atomics, allocations
on updates, or resets from an observing thread. Default builds contain none of
the counter fields or updates. The compatibility alias forwards the feature.

Existing carrier snapshot publication copies the counters; active observations
can lag. The benchmark reads **only after successful shutdown**, never between
rounds. It validates completed shutdown, stopped carriers, zero active tasks,
counter partition identities and exact remote-packet/completion totals for
one warm-up plus all measured rounds. This avoids an additional inter-round
observer scan becoming the pacing mechanism under investigation.

Output separates:

- Empty drains, full-window deferrals and nonempty admission batch sizes.
- Dispatches between idle entries, including empty episodes and the maximum.
- Early work returns, bounded poll episodes/probes/hits, and predicate waits.
- Timed wait calls and returns without observed start/wake work.

Counters include startup, warm-up, measured work and shutdown. A wait call is
**not** a count of syscalls or actual OS sleeps. An empty-work return can mean
a timer or control event; it is not proof of a spurious wake. Arrival-gap times,
actual deschedules and useful synchronization transfers are not yet measured.

The enabled feature changes layout, counter instructions and snapshot-copy
work. Its timings are diagnostic, not engine-comparison or acceptance data.
Even paired instrumented experiments require uninstrumented confirmation.

## Qualification

Source SHA-256:
`718e21c14a028feb817aae245047f103346f555e5f1616d651dff9e7998e99ce`.

All 11 canonical gates passed under receipt
`/root/.cache/zcheck/run-1788637398-489365675-2342686/receipt.json`, including
532 all-feature runtime tests (one ignored). Separate default-native workspace
qualification passed 512 runtime tests, 70 stack tests and the remaining suites.
With only `scheduler-profiling` enabled, all 517 native runtime tests passed
(one ignored). Standalone benchmark all-feature tests (36), clippy and formatting
also passed. These are Linux results, not new ARM64 qualification.

The five owner-counter tests exercise histogram boundaries, a real 64+1 admission
batch with a full-window deferral, existing snapshot publication, an epoch change
that exits after one probe, and an already-due timer that enters the wait API
without establishing an OS sleep. Three benchmark tests validate final-only
reporting, reject active/incomplete task totals, and reject count overflow.

The architecture refresh analyzes a fourth explicit feature world. It adds one
reviewed exact compiler `core::default::Default` derive allowance for zeroed
counters, using the session's standing grant approval. Other new macros use
already-approved exact standard-library paths. No dependency, debt, concurrency
or unsafe-boundary allowance changes.

## Initial observations, not an optimization result

One serial instrumented four-carrier lifecycle process completed 5.02 million
tasks (10,000 tasks, 501 measured rounds plus warm-up). It reported:

| Whole-runtime activity | Observed |
| --- | ---: |
| Remote packets and mounts | 5,020,000 each |
| Receive invocations | 4,794,077 |
| Full-window deferrals | 4,489,783 |
| Mean nonempty batch | 16.50 tasks |
| Idle entries | 3,039 |
| Dispatches per idle entry | 1,651.86 |
| Poll probes per task | 0.365 |
| Predicate wait calls | 2,835 |

These figures characterize one process with the existing capacity scan. They
do not establish the cause of the rejected scan-free lifecycle regression.
The next experiment compares these activity counts against an otherwise
identical observer-only build, with balanced independent process runs.

Thirty-two default-build controls were also captured in reversed process order
against the preserved admission checkpoint executable. The panel is **excluded
from performance acceptance**: saved host snapshots caught unrelated compiler
and architecture-check activity, and mutex/park timing and lifecycle cycles
varied materially. No speedup, zero-regression conclusion or updated May claim
is drawn from those numbers. The raw results remain available for audit.

The separate [capacity review](capacity-review.md) remains the prior rejection
evidence. No scan-free change, polling reduction or shared pending counter has
been accepted by this instrumentation slice. The pending-wake local-component
diagnostic issue and production-sized footprint are still open.

## Durable slice evidence

[`scheduler-profile-718e21c1.tar.gz`](evidence/scheduler-profile-718e21c1.tar.gz)
contains the source patch, environment, executable hashes, canonical receipt and
logs, native/benchmark qualification, exact commands, profile data, rejected
default controls and analysis script. Archive SHA-256:
`7dc52997e93f3d3499eb02afa224c35a666fb25f658397c76f6a8a5598b5b397`.
It excludes executables and is not a complete release-evidence bundle.
