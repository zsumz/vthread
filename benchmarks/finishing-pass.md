# Core finishing pass after `0b67337`

Goal: close progress defects, remove demonstrated scaling costs, then reduce
handoff overhead in separately attributable, qualified slices. This supersedes
the channel-first next-step ordering in the earlier status and experiment reports.
One runtime change is active at a time. Rejected candidates remain evidence, not
the production baseline. HTTP is out of scope.

## Frozen contracts

- No stack assembly, trampoline or execution-engine optimization; only
  qualification and demonstrated-defect repairs.
- No post-mount migration, public API redesign, scheduler rewrite or replacement
  wake mailbox.
- Preserve both cancellation checkpoints, exact generations, bounded accounting,
  selected-resource recovery, structured ownership and shutdown admission.
- Preserve ready fairness, the admission-service guarantee, the ready window and
  cached visible ingress. Do not increase parked-handoff polling.

## Ordered slices and acceptance

| Order | Slice | Required evidence |
| --- | --- | --- |
| 1 | Required default debug/release qualification; unchanged-source control | Canonical graph requires both profiles; selected engine/configuration explicit; old/mutated gate definitions fail policy; source/binary-keyed default control before runtime edits |
| 2 | Paused-publisher carrier progress | Nonblocking published/in-flight/stale query; bounded owner deferral; completion/sleep handshake; cleanup and retirement safety; composed model and ordered ordinary/mutex/active/sleeping/cancel/abandon/reuse regressions |
| 3 | Capacity scan and admission-only polling | Four arms: scan retained/removed crossed with probes existing/skipped only when no tasks are parked; tight lifecycle first, then spare lifecycle, synchronization, burst CPU, admission and tails |
| 4 | Incremental readiness | Bounded dirty/command ownership through deletion acknowledgement; backend work and wake publication outside shared metadata; bounded event/command fairness; no population scan for unchanged registrations |
| 5 | Channel attribution before another implementation | Default-build cycle profile of baseline and one identified rejected candidate; multiple work counts; existing counters separate attachment/rearm/reservation/retirement/retry; at most two implementations after attribution |
| 6 | Mutex mechanism attribution | Same-owner, remote-active and remote-sleeping recipient controls; acquisitions/suspensions/routing/native waiting/fairness; retain direct ownership, no barging |
| 7 | Six loaded findings | Capture pre-cleanup state and ordered semantics, preserve failures, distinguish harness timing from production errors; each repaired test retains a demonstrated negative control |
| 8 | Release evidence and frozen-source May panel | Native architecture, supported sanitizers, large mixed-lifetime populations, offered-load tails, footprint and idle CPU; durable source/binary/raw evidence; historical comparisons stay labeled historical |

A necessary correctness repair may have a measured cost. An optional optimization
must earn its complexity with repeatable default-build cycle gains and protected
throughput, progress, fairness, tails, footprint and busy/burst/idle behavior.

## Qualification correction

The previous handoff report incorrectly said `--all-features` selected a
compatibility engine. The source at `0b67337` contains no corosensei backend or
backend-selection feature: `vthread` directly uses `vthread-stack` in both feature
worlds. The `vthreads` package is a naming compatibility alias, not another engine.
All-features enables evidence and profiling paths. Default-feature debug/release
coverage is still required; the instrumented configuration cannot substitute for
it. The earlier report is corrected without changing its immutable raw archive.

`zcheck.toml` now retains all-feature tests and requires `test-native` followed by
`test-native-release`. The workspace suites run sequentially, not concurrently
with each other or application load. Existing final-gate dependencies remain.
The policy output identifies `engine=vthread-stack` and each configuration/profile;
five policy tests include deliberately missing gates, all-feature substitution,
missing optimization and broken dependency edges. No runtime or lock change is
needed for this slice.

The first expanded canonical attempt (`run-1788662708-282195919-2612597`)
failed the existing cancellation-history wall-time-ratio assertion after 543
runtime tests passed. Default debug/release stages were correctly blocked by
that failed dependency. The exact failure is retained; a subsequent passing run
does not classify or close that loaded-suite finding.

The second attempt (`run-1788663017-196073715-2617934`) passes all 14 canonical
gates with preserved repository state, including default debug and optimized
workspace tests. The required-configuration slice is qualified on this Linux
x86-64 host; it does not close the six loaded findings or the cross-platform
release matrix.

The initial default control was built from pristine `0b67337`, with no profiling
features, on the same external build volume as earlier measurements. Its first
ten-case panel completed. A subsequent repetition's endpoint guard detected a
host build and stopped; that sample is excluded, not silently retried or combined
into an acceptance result. This is a baseline control, not a refreshed May panel.
The [durable bundle](evidence/native-qualification-f57b1db1.tar.gz) preserves 13
completed processes (12 with clean endpoint guards, one excluded), both canonical
attempts, the tested qualification patch, source/binary hashes and replay ledger.
SHA-256: `966e766560aa17696263f1211f7b0e0ae995792d0475e8d346bf99a487e4e6a8`.
The remaining 17 planned processes did not run; they are not silently inferred.

## Stop rules and open proof obligations

The first publication slice is a [test-only composed model](publication-deferral-review.md),
not a runtime repair. It imports the real wait word and passes thirteen targeted
tests, with negative controls and explicit transport/SC/cleanup limits. Native
deferral and lifetime-safe cleanup are now an
[integrated progress candidate](publication-progress-review.md), with ordered
native regressions and shared-production-source models. The candidate passes all
fourteen canonical gates, 1,140 repeated ordered test executions and 778,182 mixed
task lifetimes. Its 104-process default cost screen records a repeatable park
penalty (about 12% local / 16% four-carrier cycles), not a performance improvement.
The necessary progress repair is retained with that explicit tradeoff; release
qualification remains incomplete.

Publisher deferral must let unrelated ready work run while publication is held;
it need not complete the paused task or its scope. Moving `publish_claim` earlier,
native yielding, or invoking a public checkpoint from half-retired state is not
a substitute for composition proof. Routes/resources stay owned until legal
retirement, including forced cleanup.

Capacity experiments preserve completion flushing and all signal/sleep checks.
They do not replace the scan with an unproven shared per-wake counter. If skipping
admission-only probes loses, one small fixed budget may be screened; no broad
adaptive search or restored scan as artificial pacing.

The [four-arm admission-only experiment](capacity-admission-review.md) is complete
and rejected at its first protected workload. Zero probes and the one fixed
32-probe follow-up both lose tight-capacity lifecycle throughput/CPU substantially.
All source and raw attempts are preserved, and the qualified runtime is restored.
Capacity-independent maintenance and correct local/deferred snapshot depth remain
open.

Readiness admission reserves eventual-removal capacity. Drop cannot block on
command space or lose deletion; cancelled in-flight installations must be deleted.
Keep descriptor ownership, subscription/token identity, level triggering, errors
and bounded shutdown. Qualify thousands of mostly idle registrations, a small
active subset, cancellation/install/event/removal races and continuous churn.

The [bounded incremental readiness experiment](readiness-incremental-review.md)
passes its ordered negative controls, production-state model, 920 repeated native
test executions and all fourteen canonical gates. At 4,096 idle registrations,
the longer control records about 82% fewer process cycles and 86% less exchange
time. Its protected CPU/cycle/tail panel does not establish non-regression, so
promotion is held on `perf/readiness-incremental`. The production perf branch
keeps the retained readiness implementation. All 192 completed counter processes
and the complete candidate are archived. No polling/layout rescue follows this
mixed result; default-build channel attribution is the next independent slice.

Channel screening is single-carrier forced handoff, four carriers/eight tasks,
four/64, then endpoint tails/burst CPU and full qualification. Preserve immediate
success, selected FIFO ticket accounting, buffered payload ownership and RAII wake
publication. No sixth layout permutation without the missing attribution pass;
after two justified failures, record the result and move on.

The [matching historical A/E attribution pass](channel-attribution-review.md)
now preserves default cycle profiles, 48 complete established-counter processes
at two work counts and twelve extended diagnostic processes. E adds roughly one
local rearm reference-count pair per value while both retain an extra unsuccessful
retry crossing. Profiles have explicit sampling/throttling limits; an expanded
counter timeout and final build-overlap exclusion remain visible. No new channel
implementation is attempted or promoted. Exact mutex owner/active/sleeping
attribution remains independent; direct ownership and polling stay frozen.

During that next measurement-only slice, canonical qualification exposed a
[publication observer ordering assumption](publication-observer-review.md).
The test-only repair accepts the two exact route/deferral observations in either
arrival order, without changing publication or retirement. Its deterministic
negative control fails the old assumption; all fourteen gates and 400 additional
native sleeping-owner tests pass, including 34 owner-first observations. The
mutex fixture is preserved separately and has not yet produced a performance panel.

The [mutex mechanism fixture](mutex-mechanism-review.md) is now qualified under all
fourteen gates. It verifies measured-task ownership and separates local,
remote-active and observed-sleep handoffs without changing runtime semantics.
Its final CLI smokes pass, but both independent-panel attempts stopped before
their first process on an unrelated build. The 18 default/nine diagnostic panel
is explicitly pending, not replaced by smoke medians. Ordered loaded-finding
repairs are the next independent work; no ownership or polling candidate follows
these incomplete performance observations.

A [frozen-source mutex resumption](mutex-mechanism-partial-review.md) now records
twelve completed default processes, eleven with clear build guards, before another
guard stop. Six default and all nine diagnostic processes remain unrun. The partial
long controls distinguish roughly 190 ns local / 330 ns remote-active medians from
10.5 us after observed sleep, while retaining maxima near 19 ms. These forced-state
controls do not establish production state frequencies or a new May comparison.
No mutex or polling change follows the incomplete panel.

The [readiness retry finding](io-retry-order-review.md) is closed as a test-oracle
defect. Ordered real-kernel/socket regressions cover materialized peers, the exact
128-yield boundary and legal admission after selection. Three negative controls,
fourteen canonical gates and 300 additional one-CPU native executions qualify the
test-only repair; no retry or polling behavior changes.

The [cancellation-history test split](cancellation-history-order-review.md)
preserves mandatory 100,000-generation semantic bounds and both cancellation
paths while retaining the unchanged timing allowance in an explicit optimized
`zcheck` task. Pruning, missing-handoff and disabled-threshold negative controls,
fourteen gates and 1.2 million further constrained successor generations pass.
Its single guarded timing-task invocation passes, but does not reconstruct or
explain the historical excursion; that remains release-performance evidence.

The [selected-timer ordering repair](selected-timer-order-review.md) now observes
the exact published generation before delayed resumption, tests equal-deadline
policy, and demonstrates valid late admission with no park. Four negative
controls, fourteen canonical tasks and eighty additional ordered subcases pass.
No runtime timer or checkpoint behavior changes; the old unlabeled timeout does
not identify a unique historical schedule.

The [cross-runtime join repair](cross-join-order-review.md) replaces native test
gates with exact deadline selection, typed interruption/handle return and one-time
result recovery across distinct runtime identities. An ordered completed-panic
case demonstrates the old deadline-only oracle's invalid assumption. Three
negative controls, fourteen gates and eighty further native executions pass;
production join/ownership behavior is unchanged.

The [deadline-first join repair](deadline-join-order-review.md) now holds an exact
selected notice while the child is reclaimed, then verifies deadline selection
and one-time result recovery for both transferable and borrowed handles. A third
ordered case preserves valid local deadline policy alongside an unused observer's
disconnection. Three negative controls, fourteen gates and 120 further native test
executions pass. Join/wait, timer, ownership and ready policy remain unchanged.

The [inbox evidence repair](inbox-refill-evidence-review.md) now captures accepted,
queued, started and completed work before cleanup can produce secondary rejection.
Lost-notification and cleanup-order negative controls, fourteen canonical gates,
forty targeted repetitions and four full oversubscribed suites pass. No production
change follows. The historical refill stall remains the last unclassified loaded
correctness release blocker; its missing state cannot be reconstructed by reruns.

| Loaded finding | Current disposition |
| --- | --- |
| Inbox refill | Pre-cleanup evidence repaired and qualified; historical stall still unclassified and release-blocking |
| Cross-runtime join | Ordered deadline/handle recovery and completed-panic controls qualify the test-oracle repair |
| Deadline-first join | Exact selected notice retained through child completion; owned/borrowed recovery and unused-observer counterexample qualify the test repair |
| Selected timer | Exact selection before delayed mount and late-admission counterexample qualify the test repair |
| Cancellation history | Semantic bounds and timing gate separated with negative controls; historical timing excursion still open |
| Readiness retry count | Ordered materialization, retry boundary and late-admission controls qualify the test-oracle repair |

The historical cancellation-history timing excursion remains separately open.
A longer timeout or quiet rerun is not closure. Native ARM64, sanitizer support
and the full mixed-lifetime release matrix remain separate from a local canonical
pass.

The [fixed offered-load application control](offered-load-review.md) now records
all scheduled arrivals independently of completion, uses bounded persistent
client slots, and separates generator lag, accepted-request tails and client
backpressure. Nineteen evidence tests, deliberate schedule/backpressure/tail
mutations, 700 repeated ordered tests, a final one/four-carrier matrix and all
fourteen canonical tasks pass. This is a Python-harness qualification slice with
no runtime change, not controlled-host performance or large-lifetime acceptance.
