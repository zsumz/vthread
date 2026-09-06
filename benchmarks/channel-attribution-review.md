# Channel attribution: a concrete rearming cost, no promoted candidate

This completes a missing default-build profiling/diagnostic pass against the
matching historical baseline and rejected inlined variant E. It does not refresh
current-head performance or May. The production perf branch keeps the qualified
publication-progress repair; no channel, polling, ownership or placement change
is retained here. Out-of-lock channel publication remains open.

## What the counters establish

Both default binaries are the original saved release builds from the
[channel-publication review](channel-publication-review.md), based on `3161514`.
A's binary hash begins `1680cc61`; E's begins `4006e9fa`. E's source is
`56f61f8642ebd8f12599096f91fefa7c5dabad649f5d9ae7b11c2ab95aa84668`.
The archive preserves complete hashes, original source reconstruction and exact
commands. These builds contain symbols but no handoff timing instrumentation.

Four AE/EA/EA/AE process pairs per case use the established five hardware/software
events, normal admission, exact delivery checking and verified carrier pinning.
One carrier uses CPU 7; four use CPUs 0-3. All 48 processes complete with clean
endpoint build guards. This does not prove physical-host isolation.

| Case | Values/process including warmup | A ns/value | E ns/value | Process cycles | Process instructions |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 carrier / 8 tasks | 320,000 | 426.24 | 434.24 | +2.07% | +4.24% |
| 1 carrier / 8 tasks | 3,200,000 | 780.11 | 797.01 | +3.10% | +4.31% |
| 4 carriers / 8 tasks | 320,000 | 1,517.30 | 1,170.05 | -3.95% | -0.76% |
| 4 carriers / 8 tasks | 3,200,000 | 1,413.35 | 1,297.69 | -7.64% | -0.63% |
| 4 carriers / 64 tasks | 512,000 | 1,372.49 | 1,253.57 | -5.81% | -0.02% |
| 4 carriers / 64 tasks | 5,120,000 | 1,326.60 | 1,200.10 | -11.54% | -1.89% |

All four long local cycle pairs lose, by 2.71-3.51%. The extra local cost is not
just a fixed setup charge. Two-count secants remain whole-process observations,
not isolated handoff costs or a linear model; duration and cache footprint change.
Ns/value is throughput-derived, not individual latency. The remote improvements
also mean E's earlier remote losses cannot be treated as universal constants.
No new protected burst/idle/tail acceptance claim follows the local loss.

## Attribution and sampling limits

The first counter group added branch/branch-miss events. It completed 25 processes,
then timed out the first long remote E run at 120 seconds. Only 24 completed runs
form pairs. Its remote behavior is far outside the established-counter panel;
it is retained separately. Event-set causality is not proven by these sequential
groups, and the rejected binary's timeout is not established as a production
deadlock. It is not silently converted into a throughput sample.

The first 12 cycle recordings exceed the host's 500-Hz sampling ceiling and are
heavily throttled. A second 12-recording pass requests 199 Hz and supplies more
than 11,000 local samples across both arms, with symbols and annotated assembly.
It has no lost-record markers but still contains 29-31 local and 117-152 remote
throttle records per process. Consequently the profiles are directional evidence,
not complete CPU coverage or precise incremental instruction costs. A final
fixed-low-rate attempt is excluded after its first recording overlaps a host
build; no complete pair or hidden replacement run is claimed.

The useful findings are narrower than “Arc caused the entire loss”:

- Local ticket enqueue accounts for about 3.07% of A's sampled period and 4.35%
  of E's. E's annotated samples concentrate immediately after reference-count
  increments on both initial enqueue and rearm.
- E's publication-array destructor samples concentrate after reference-count
  decrements. Its entire symbol share is not added overhead: useful publication
  work moves out of A's channel body, and old ticket/finish symbols lose work.
- Remote channel bodies account for roughly 40-48% of sampled period, with hot
  metadata-spinlock acquisition/backoff regions. The carrier loop accounts for
  roughly 23-29%, concentrated in existing polling. Hub operations take about
  7-9%. This does not justify a backoff/polling change or establish exact owners
  from shared-hub routing.

Sampling skid and incomplete coverage prevent assigning exact instruction costs.
No execution-engine change follows these profiles.

## The missing event counts

Two isolated historical checkouts extend the existing owner-local profiler with
attachment, initial enqueue, rearm attempt, rearm clone, ticket consumption,
cleanup removal and selected-generation finish-retirement counts. Existing
notification/park/retry/transfer counters remain. No new clock framework or shared
hot counter is added. Twelve instrumented processes are diagnostic only.

The two local repetitions agree exactly:

| Event | A | E |
| --- | ---: | ---: |
| Useful values | 32,000 | 32,000 |
| Initial ticket enqueues | 63,996 | 63,996 |
| Rearm attempts | 31,996 | 31,996 |
| Rearm handle clones | 0 | 31,996 |
| Actual suspensions / selected retirements | 95,992 | 95,992 |

Both perform approximately three suspensions per value, including one unsuccessful
resumed retry. E adds approximately one reference-count increment/decrement pair
per value through rearming and publication-handle release. It does not remove
that extra scheduler crossing. E also coalesces notifications that A stores as
permits, changing finish work; the additional pair is a demonstrated component,
not an explanation of every measured difference. Remote diagnostic event ratios
vary and cannot be substituted for default-build counts or topology evidence.

Each probe passes two native accounting tests and one report test. Deliberately
misclassifying rearm as initial enqueue fails the real-channel assertion (4 versus
3); restoring identical source passes. Three analysis tests reject failed,
contaminated and multiplexed runs and preserve unrounded sample periods.
The feature-on patches remain isolated; no canonical candidate or new full
protocol/architecture qualification is claimed.

## Decision and evidence

No sixth channel implementation is attempted. The permitted two-implementation
budget remains unused. The evidence identifies redundant retry/refcount work,
but does not qualify another layout permutation or resurrect the rejected
eligibility/polling experiments. Exact same-owner, remote-active and
remote-sleeping mutex attribution remains the next independent performance
investigation. Direct ownership stays unchanged. The six loaded findings remain
separate release blockers.

The [durable bundle](evidence/channel-attribution-56f61f86.tar.gz) has SHA-256
`1159959bd818adf21ee07aefa0eb954a16b8b8053326542443984beb8de3e38c`;
all 1,280 internal hashes verify. It preserves raw counter/profile attempts,
sampling limits/throttling, excluded timeout/build overlap, diagnostic patches,
negative and passing tests, source/binary identities and replay/analysis scripts.
Executed binaries and temporary worktrees are excluded. This is slice evidence,
not a release bundle or a refreshed May comparison.
