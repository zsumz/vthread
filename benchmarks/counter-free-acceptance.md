# Counter-free recovery pass after `d83b787`

Goal: recover demonstrated scaling improvements without sacrificing useful
handoff throughput, progress, CPU consumption or tails. This is the revised work
order for the unfinished core finishing goal, not a release-completion claim.
The historical counter-collected decisions remain intact and labeled by mode.

## Frozen scope and order

One production candidate at a time. Keep native stacks, both cancellation
checkpoints, post-mount affinity, exact generations, bounded ownership/accounting,
publisher completion interest, ready/admission fairness, direct mutex ownership
and the existing 640-probe policy. No HTTP or public API changes.

1. Reproduce scan-only capacity candidate B. Its production delta removes the
   remote depth scan from carrier publication and adds fresh remote depth to the
   published local/deferred contribution during explicit observation.
2. Reproduce the existing bounded incremental-readiness candidate (`f127ed1`),
   preserving its installation/removal proof and deletion acknowledgment.
3. Bound work before dispatch only after measuring burst/deferred service costs;
   prove persistent continuation without new signals or paused-publisher spinning.
4. Reproduce channel suspension/rearm ratios and evaluate the existing eligible-
   grant design before another implementation. Target the unsuccessful crossing.
5. Attribute production mutex useful-handoff stages before changing that path.
6. Qualify a frozen source across architecture, sanitizer support, large live
   populations, footprint, CPU, offered-load tails and competitor controls.

The first two items are acceptance re-evaluations, not permission to repeat the
zero/32-probe experiments or invent another reactor. A rejected candidate must
leave the retained runtime unchanged. A correctness repair is judged separately
from an optional speed optimization.

## Predeclared acceptance (before new A/B timings)

Use fresh default-feature release processes without profiling/allocation hooks.
Compare A (retained runtime) and B (one candidate), separately in bare and
`perf stat` modes. Preserve every planned outcome, including failures, timeouts,
overlap guards and unrun jobs. No replacement of unfavorable samples.

Within each case and mode, balance A-first/B-first order. Also balance bare-first/
counter-first order. Save exact commands, source/configuration and executable
hashes, toolchain/topology, raw output, runner and analysis tests. Do not combine
within-process rounds with independent process replications.

- Early screen: four paired processes per collection mode and placement. Stop
  this candidate before a broad panel if bare time regresses by more than 10%
  in at least three pairs and the median paired ratio exceeds 1.10. Correctness,
  progress or accounting failures stop immediately, regardless of timing.
- A surviving candidate gets twelve new paired bare processes per protected
  cell. Meaningful non-regression margin: 5% for throughput-derived time and
  whole-process CPU per useful operation; 10% for endpoint p99.9/p99.99 and
  worst-task latency where provided. Report both median ratios and a one-sided
  95% upper bootstrap bound on the paired geometric-mean ratio (10,000 resamples,
  fixed seed 20260906). Promotion requires the upper bound within the margin.
  Inconclusive means held, not passed; do not keep sampling until green.
- Intended scaling workload: at least 50% counter-free median time reduction at
  4,096 idle registrations for readiness, or at capacity 65,536 with 64 live
  synchronization tasks for scan removal, plus an algorithmic regression test.
  Throughput alone cannot waive protected-workload, CPU, tail or progress gates.
- Counter-mode candidate/baseline ratios are diagnostic evidence, reported next
  to bare ratios. A counter-mode loss does not veto a bare pass automatically;
  it triggers explicit collection-interaction analysis. Nor does a counter-mode
  win rescue a bare failure. Whole-process cycles include setup/teardown.
- Obtain bare process CPU/context switches/peak RSS from post-exit resource
  accounting, without a live sampler or PMU collector. Report these separately
  from wall time and endpoint latency. Burst/idle and offered-load controls must
  pass their intended-work/accounting checks and the same CPU/tail margins.

For scan B, screen tight-capacity lifecycle first: four carriers, 1,000 tasks,
501 measured rounds, process mask 0-3, separately default and `--pin-carriers`.
Then protect 10,000-task lifecycle, spare capacity 65,536, park/channel/mutex at
8 and 64 tasks, wake/TCP tails, burst/idle CPU, admission progress and offered load.
Do not increase handoff polling or alter the ready window to rescue a loss.

For readiness, start with protected small-active TCP and lifecycle at tight and
spare capacity. Then measure thousands of mostly idle registrations with a small
active subset, sustained churn, full-capacity re-registration, burst/idle and
offered-load tails. Reuse the existing population and application fixtures.

Keep default placement results separate from individually pinned controls.
The final May panel retains May's default behavior; record actual execution
placement in diagnostics. Disabling stealing is not an affinity-equivalent
control. May park timeouts remain a separate unresolved control investigation.

## Snapshot contract for scan B

Carrier progress publication is capacity-independent: record local queued plus
consumed-but-deferred wake notices, without scanning remote slots. An explicit
runtime snapshot separately observes remote depth and adds it once to the cloned
published value. Explicit observation may still scan provisioned remote capacity.

This is a weakly consistent diagnostic sum, not an atomic occupancy measurement.
Local/deferred values come from the last carrier publication (batched at 64
counted transitions, plus idle/terminal publication); remote depth is observed
later. Concurrent movement between components can transiently over/undercount.
Repeated observations must not accumulate depth; quiescent boundaries must retain
all three contributions. No scheduler decision may rely on this approximate sum.

Negative controls must show that the retained code both scans during publication
and hides local/deferred contributions. Normal remote refresh remains a positive
control. Polling, completion flushing, ingress caching and sleep ordering remain
byte-for-byte unchanged in production.
