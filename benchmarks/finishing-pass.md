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

The six loaded findings remain unclassified release blockers: inbox refill,
interrupted cross-runtime join, delayed selected timer, cancellation-history
timing, readiness retry count and deadline-first join. A longer timeout or quiet
rerun is not closure. Native ARM64, sanitizer support and the full mixed-lifetime
release matrix remain separate from a local canonical pass.
