# Core vthread performance status: 2026-09-06

This is the consolidated status of `perf/scheduler-hot-path` at `ff45f2b`, not a
new benchmark run. Production source remains the qualified `3161514` baseline,
SHA-256 `f57b1db1457fc45f855e561e5eb89e489c72bcf166ef4c50400596057840c976`.
The subsequent channel-publication checkpoint preserves experiments and evidence;
it does not ship a channel optimization. HTTP remains a separate project.

## Bottom line

Execution and task lifecycle have strong measured results. Multi-carrier resource
handoff, provisioned-capacity costs, loaded tails and readiness scaling are not
finished. Recent work has repaired correctness and progress defects, improved
measurement, and rejected changes whose local wins hid other costs. We have not
established an across-the-board May win or stable-release readiness.

## What is in production

The retained foundation is the native stack engine, fixed post-mount carrier
affinity, compact carrier-owned task storage and eight-byte ready indices,
resident wait state, local wake routing, stack/execution reuse, and direct-handoff
mutex ownership with cancellation recovery. Bounded resources, structured scopes,
generation checks and both cancellation checkpoints remain part of the contract.

The follow-up review has produced these separate repairs and evidence slices:

| Area | Landed result | Limit or cost |
| --- | --- | --- |
| ARM64 terminal transfer | Return the outcome in `x0`; real ARM64 and x86-64 native-stack CI passed in debug and release at the repair revision | Full ARM64 runtime/stress matrix and durable raw CI archival remain open |
| Ready fairness | Independent normal/oldest-wake service in two-wake cohorts; a persistent queue head gets service within four dispatch opportunities | Cooperative bound, not wall-clock guarantee; explicit median/p95 and cycle tradeoffs |
| Admission fairness | Pending admission advances across park as well as yield; quota expiration admits one packet | The full-window quota is still 65,536 dispatch opportunities, not a low-latency promise |
| Stall/service tests | Captured the actual failure ordering and replaced watchdog timing assumptions with ordered evidence | Six additional oversubscribed-suite findings remain unresolved |
| Visible ingress | Remember already-visible ingress without depending on the producer completing its notification | Does not remove capacity scans or repair the wait-claim publication dependency |
| Maintenance | Borrowed revocation is epoch-gated with exact occupancy; expired timers identify the task directly | Capacity-dependent wake-depth observation remains |
| Measurement | Shared bounded MPMC control, sampled endpoint tails, opt-in handoff/idle attribution and replayable bundles | Instrumented and sampled workloads are not interchangeable with default throughput runs |

See the [finish-line plan](finish-line-plan.md),
[ready fairness](ready-fairness-review.md),
[admission fairness](admission-fairness-review.md),
[ordered wait qualification](wait-qualification-review.md),
[ingress repair](ingress-visibility-review.md), and
[handoff attribution](handoff-attribution-review.md).

The ready-fairness panel reduced vthread wake p99.9 from roughly 141–143 us to
16–17 us, with a roughly 30–39 ns median increase and worse p95. This was a separate
vthread-only experiment, not a fresh paired May comparison. It is an explicit
fairness tradeoff, not a claim of improvement at every percentile.

## Where we compared with May

The last complete panel is the [2026-09-05 checkpoint overview](checkpoint-overview.md),
based on the borrowed-occupancy work-in-progress source after `c7beec5`. It predates
the later fairness/progress repairs. **These are historical results, not current-HEAD
measurements.** Most rows use four carriers and 64 tasks on an eight-vCPU KVM guest,
with tightly provisioned task capacity. The overview records exact commands,
source/binary identity, process ordering, host limitations and operation contracts.

| Case | Direction in that panel | Important boundary |
| --- | --- | --- |
| Yield | Vthread about 1.3x faster | Throughput-derived ns/yield, not individual switch latency |
| Spawn/complete/reclaim, 1,000 tasks | Vthread about 7.3x faster | Whole lifecycle; VM/protocol-specific |
| Spawn/complete/reclaim, 10,000 tasks | Vthread about 16.5x faster | Wide round tails in both engines |
| Uncontended mutex | Vthread about 1.4x faster | One carrier / one task |
| Park handoff | May about 1.5x faster | Fixed-affinity vthread versus migrating May |
| Contended mutex | May about 2.3x faster | Ownership-transfer and scheduling costs remain |
| Historical paired channel | May about 1.9x faster | Bounded-one vthread versus unbounded May MPSC |
| Capacity-one SPSC control | May about 5.8x faster | General bounded vthread versus May SPSC plus semaphore; not identical full contracts |
| Timestamped wake throughput | May about 1.7x faster | Includes timestamping; a stored permit need not suspend |
| Wake-to-resume median | Vthread about 2.4–3.2x faster | Old vthread p99.9 was much worse; later fairness results are a separate panel |
| TCP whole-round throughput | May about 1.16x faster | Eight clients; includes peer lifecycle |
| TCP individual median | Vthread about 1.1x faster | Vthread p99.9 was worse; not readiness-scale or HTTP evidence |

No subsequent channel or capacity experiment reran that complete May panel.
Recent cycle deltas compare vthread candidates with vthread baselines. The older
comparison's temporary raw paths are also not a durable release-evidence bundle.

## What we tried and did not ship

| Experiment | Useful finding or win | Why it did not qualify |
| --- | --- | --- |
| Wake-mailbox integration, two variants | Modeled stale-generation/sleep boundaries and measured a replacement route path | Four-carrier mutex and park regressed; production retains the existing bounded wake queue |
| Mutex ownership bookkeeping alternatives | Small local cycle reductions in some controls | No repeatable multi-carrier win; an unconditional ownership-swap variant failed its model and was not timed |
| Observer-only capacity scan removal | Large spare-capacity park improvement; roughly 1,325 to 97 ns/op in one panel | Tight lifecycle cycles increased about 31%, with smaller admission batches and much more polling |
| Scan removal plus the ingress repair | Removed millions of empty idle reentries | Tight lifecycle cycles still increased 24.58%; removing the scan alone is insufficient |
| Eligible notifications plus fixed longer polling | Large shared-channel throughput/cycle gains | CPU between bursts rose to roughly 2.6x baseline |
| Adaptive longer polling | Shared-channel cycles fell about 32–37% | Sampled p99.9 worsened and 100 us burst CPU increased 26.1% |
| Cancellation-safe channel publication, five variants | Reservation under lock, RAII wake publication outside it, native race tests and production-adapted models | Every implementation missed the combined cycle/throughput/tail screen |

Evidence and exact qualifications:
[mailbox](mailbox-integration-review.md),
[mutex ownership](mutex-ownership-review.md),
[capacity/pacing](capacity-pacing-review.md),
[fixed polling](channel-tail-review.md),
[adaptive polling](adaptive-idle-review.md), and
[channel publication](channel-publication-review.md).

### Latest channel slice, specifically

All five prototypes keep the FIFO ticket counted until consumption or cancellation
cleanup and leave payload ownership in the buffer. They reserve the selected wait
generation under the metadata lock, then publish outside it through an owned RAII
guard. Cancellation, close, panic, delayed publication and generation reuse are
explicit tests, not assumed-safe windows.

The first variant reduced single-carrier cycles 16.7%, but increased four-carrier /
eight-task cycles 287.8% and sampled send p99.9 135.3%. Later variants avoided that
collapse but added roughly 9% single-carrier cycles. The final inlining-only variant
improved sampled receive/send p99.9 from 53.19/52.04 us to 25.13/23.91 us, while
unsampled single-carrier cost rose from 418.94 to 458.24 ns/value. Its four-carrier
cycle costs also increased. Inlining did not recover the regression.

The durable [channel-publication bundle](evidence/channel-publication-56f61f86.tar.gz)
contains 203 completed counter processes: 202 in complete analysis pairs and one
explicitly excluded unpaired process. It preserves patches, source/binary hashes,
negative controls, failed and passing models/tests, raw results, analysis and the
restored-baseline canonical receipt. Archive SHA-256:
`888f1481a6d90424877022aaaf0b1fc77b70ab78e10f637c3e892b6166e043dc`.
All 1,503 file hashes and the reproduced analysis were verified before publication.

The final protocol design passed 13 model tests and 23 feature-on native
channel/accounting tests. The inlining-only follow-up was rejected before separate
full native/canonical candidate qualification. Models cover explicit bounded
selection/retirement/reuse cases, not native signaling and the entire runtime.
The designs are replayable research, not release-qualified implementations.

## Next: one channel slice, without changing idle policy

1. Isolate the extra single-carrier reservation, retirement and rearming work.
   The code-generation audit removed one outlined helper without removing the
   cycle penalty; that is not a completed attribution.
2. Reuse the ordered native regressions and production-adapted ownership models.
   Keep selected tickets counted, retain cancellation recovery and RAII publication,
   and do not introduce direct cross-stack payload transfer in this slice.
3. Screen uninstrumented binaries in balanced independent process pairs. Require
   local cycle improvement before wider qualification; reject another small-remote
   sleep collapse, worse loaded tails/fairness, or increased idle/burst CPU.
4. Only after that screen passes, run unaffected workloads, tight/spare capacities,
   wider topology and full canonical/native qualification. Publish source-keyed
   evidence before making a new speedup claim.

No scheduler-policy change is bundled into this experiment to rescue a channel
candidate that fails its own screen.

## What remains after that

- **Capacity-independent maintenance:** remove recurring provisioned-slot scans
  while deliberately preserving useful admission batching and idle pacing. Gate
  64 live tasks at capacities 64, 1,024 and 65,536 or larger, plus tightly/sparely
  provisioned lifecycle, burst CPU and tails. Capacity-sized metadata footprint and
  the local-plus-remote diagnostic depth issue remain open.
- **Mutex and wake round trips:** attribute local, remote-active and remote-sleeping
  transfers, selected cancellation, waiter age and per-task fairness. Preserve the
  existing direct-ownership protocol. The [publisher-pause dependency](publication-review.md)
  is characterized, not repaired or established as the cause of every measured tail.
- **Incremental readiness:** bounded add/remove/rearm processing proportional to
  changes/events, replacing recurring full-map reconciliation. Then qualify many
  connections, churn, cancellation, descriptor reuse, retry/syscall counts, CPU
  and loaded tails. Eight-client loopback results are not this proof.
- **Release correctness:** close the six outstanding loaded-suite findings with
  observed ordering: coalesced inbox refill, interrupted cross-runtime joins,
  delayed selected timers, cancellation-history timing, readiness retry counts,
  and deadline-first joins. They are unresolved findings, not six established
  production defects and not waived because quiet reruns passed.
- **Release evidence:** archive exact-revision cross-platform native logs; qualify
  full native runtime behavior separately from all-features compatibility builds;
  document sanitizer/alternate-stack support; finish replayable large-population
  mixed-lifetime stress, memory, loaded tails, worst-task progress and idle CPU.
  A full 10-million mixed-lifetime release qualification remains open.
- **Fresh May comparison:** after an accepted source baseline, rerun balanced
  default-contract and controlled-mechanism panels with durable raw evidence.
  Preserve backpressure, placement, sampling and VM caveats instead of merging
  unlike measurements into a single victory claim.

The restored production source passes all 11 canonical `zcheck run check` gates
under receipt `run-1788659610-751414318-2581158`, archived in the channel bundle.
That is baseline qualification, not approval of the rejected prototypes or proof
that the release checklist is complete. No source, architecture grant or runtime
policy changes are part of this status report.
