# Scan-only maintenance: counter-free rejection confirmed

Decision: **do not promote scan-only B**. It fails the predeclared tight-lifecycle
early screen without counters, under both default and individually pinned
placement. This re-evaluation does not dismiss the production-capacity problem;
it narrows the next investigation to admission/service frequency rather than
assuming collection or additional native sleeps explain the whole loss.

Base `2ee55ed` retains source `e344a60e`; the rebuilt default executable exactly
matches the previous quiet-window binary. Candidate source:
`628c9522113518442a2285fb01eedb31489342f42fb76972cebc46650ca5e3c6`.
The candidate restores the two production expressions from historical B: omit
remote scanning during carrier publication, and add fresh remote depth to the
published local/deferred contribution during explicit snapshots. No polling,
fairness, ingress, stack, ownership or publication-order change is included.

## Completed acceptance screen

Four independent A/B pairs per row, 4 carriers, 1,000 tasks, 501 measured rounds,
mask 0-3. A/B order and bare/counter order are balanced. All 32 planned processes
complete; endpoint build guards are clear. The acceptance rules were committed
before running either arm. Ns/task is whole-round throughput-derived time.

| Placement / collection | A median ns/task | B median ns/task | Median paired time change | Median paired process CPU change |
| --- | ---: | ---: | ---: | ---: |
| Default / bare | 384.88 | 443.05 | +16.38% | +18.99% |
| Default / perf stat | 424.88 | 478.47 | +12.62% | +19.77% |
| Individually pinned / bare | 903.71 | 1,115.69 | +23.58% | +24.63% |
| Individually pinned / perf stat | 412.05 | 491.50 | +19.46% | +19.70% |

Paired changes are not ratios of the two unpaired medians. Default bare has three
of four paired losses above 10%; pinned bare has four. Both exceed the early-stop
rule, so no broad protected/scaling confirmation panel is run. A counter-mode loss
did not determine this decision. Collection changes absolute execution time
substantially, particularly in the pinned control, but does not reverse B's loss.
This is evidence of mode sensitivity, not an established PMU/kernel cause.

Post-exit CPU accounting includes process setup/teardown and timeout/perf wrappers.
It is separate from bare timing, not an isolated per-handoff cost. Reported RSS is
a process-tree high-water observation, not a runtime-only footprint qualification.

## Existing scheduler counters: the useful next clue

Sixteen further processes use only the existing `scheduler-profiling` feature:
four A/B pairs for each placement, with all 502,000 task lifetimes and counter
identities checked. These instrumented observations do not qualify performance.

| Default-placement diagnostic | A median | B median |
| --- | ---: | ---: |
| Tasks per nonempty receive batch | 2.025 | 1.006 |
| Receive calls | 250,736 | 656,645 |
| Idle entries | 31,823 | 476,487 |
| Poll episodes | 31,311 | 473,606 |
| Poll probes | 2,223,450 | 10,861,646 |
| Native wait calls | 2,024 | 2,031 |

The small batches and much more frequent idle/service work reproduce after the
cached-ingress repair. Default bare voluntary context switches are also nearly
unchanged: process medians 3,592.5 versus 3,566.5. Therefore an across-the-board
"scan removal causes more native sleeping" explanation is unsupported. Pinned
diagnostics do show more waits (24,949 to 35,600); placement matters.

The next capacity design should reduce demonstrated empty/near-single-packet
service and publication overhead while preserving prompt admission. These counts
do not yet isolate control-lock publication cost or establish a specific repair.
No artificial pacing, new polling budget or shared per-wake counter follows this
result. Capacity-independent maintenance remains required and unresolved.

## Correctness and retention

The ordered old-code negative controls fail precisely: four remote-depth scans
during explicit/batched/terminal carrier publication; omitted consumed/deferred
depth; repeated combined local/remote observations returning one instead of two.
The remote-only refresh control passes. With B, all four tests pass, including
paused publication and repeated observation without accumulation.

Both default benchmark builds pass their 47 tests, formatting and clippy. Seven
analysis tests pass after a pre-measurement import-path correction. No full B
canonical run is claimed: this optional candidate stopped at its first screen.
The unchanged retained runtime is restored. The complete candidate remains in
the source archive and stash `baa7aa284043eddda593402a3f1c154190dcefad`.

The [durable bundle](evidence/scan-counterfree-628c9522.tar.gz) contains 520 verified
internal hashes, complete A/B source bytes with reconstructed source hashes,
binary identities, all raw attempts, predeclared rules and replay scripts.
SHA-256: `4abcbb9de138beb481ef8a982dd7b11ffa8adc98962b983d04a19c2fbd7f0c93`.

Next independent candidate: the existing bounded incremental-readiness
implementation, evaluated counter-free against the retained correctness baseline.
No new May result or release-readiness claim is made by this slice.
