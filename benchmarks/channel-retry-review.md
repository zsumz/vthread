# Eligible channel turns: useful work removed, capacity screen fails

**Reject this optional candidate under the predeclared counter-free stop rule.**
It removes the local unsuccessful retry and improves many measured workloads,
but loses the default-placement, 64-live-task, capacity-1,024 protection cell.
The production runtime is restored unchanged. There is no new May comparison.

Base is pushed `028d40a`, native source `d38dcb86`, executable `6c332ada`.
Candidate B is historical eligible resource-grant candidate A from the
[publication experiment](channel-publication-review.md), rebased onto retained
publisher progress. Its source is `349bc73a`, executable `0de6d365`. This is not
another channel layout, polling experiment or mutex rewrite.

## Mechanism and correctness boundary

The candidate keeps the FIFO entry's identity and waiter accounting while moving
its existing owning wait handle into a resource-publication guard. Eligible
directions are selected under metadata; routing happens outside it. Ordinary
publication uses two stack slots, not a heap batch. Close/endpoint destruction
uses bounded cold batches. Payloads stay under the channel lock; no arbitrary
value crosses stacks directly, and no raw pointer replaces owning references.

Immediate success still avoids blocking wait machinery. Selected cancellation
preserves value/input ownership, removes the retained ticket under the same lock,
and makes a successor eligible. The existing permit finish consumes the selected
turn while preserving both cancellation checkpoints. Rearming clears only a
retired generation's idle grant. Affinity, generations, completion interest,
ready/admission fairness, mutex ownership, scans and 640 probes are unchanged.

The default bodies of channel.rs, core.rs and endpoints.rs match the archived
eligible variant's Git blobs. Rebase differences are current private imports,
the existing seven-event ticket diagnostics and regression/model adapters.

## The extra crossing disappears

The clocked profiler distinguishes attachment, first enqueue, rearm, rearm clone,
consumption, cleanup and retirement. Retirement includes both plain-notification
and permit finish paths. Two diagnostic processes per arm/shape use 1,000 values
per producer, seven measured rounds plus warmup, with individual carrier pinning.

| Shape | Retained actual parks/value | Candidate actual parks/value | Retained retries/value | Candidate retries/value |
| --- | ---: | ---: | ---: | ---: |
| 1 carrier / 8 tasks | 2.9995 in both | 1.99975 in both | 0.99975 in both | 0 |
| 4 carriers / 8 tasks | 2.5424–2.7846 | 1.8693–1.9815 | 0.5858–0.7873 | 0 |
| 4 carriers / 64 tasks | 2.5472–2.5962 | 1.9999 in both | 0.5473–0.5963 | 0 |

No candidate rearm clones occur. Each local process checks 32,000 useful values:
95,984 retained versus 63,992 candidate actual suspensions. API park calls/returns,
retirements and kernel crossings remain distinct checked quantities. These
clocked timings are not default-build acceptance. Native wait API counts do not
establish the recipient's sleeping state at publication.

Candidate notification totals count owned successful selected/stored publications,
not every rejected reservation or inspection of an already-selected entry. The
value, actual-suspension and rearm comparisons do not require treating those
totals as identical notification-attempt counters.

## Counter-free screens

All 264 planned performance processes complete without replacement: 32 local,
64 small-remote, 64 population, 40 endpoint-tail and 64 capacity/buffer controls.
The first three stages have four balanced A/B pairs separately in bare and PMU
modes; the latter stages have four new bare pairs. There are 184 bare and 80 PMU
processes, plus twelve separately labeled clocked diagnostic processes.

Default release builds have no profiling/allocation hooks. Each process uses 101
measured rounds plus warmup. The MPMC denominator counts producers, half the task
population. Local runs use CPU 0; four-carrier runs use CPUs 0–3. Individual
pinning and default placement remain separate. Headline rounds do not collect
owner traces. Clear build guards on this eight-vCPU EPYC KVM guest do not prove
physical-host exclusivity. Compiler, topology and exact commands are archived.

Changes are median paired ratios, not ratios of unpaired medians. Time is
whole-round throughput-derived time, not individual-operation latency. Post-exit
process CPU includes setup, warmup, validation and teardown.

| Carriers/tasks, values per producer | Bare time | Bare process CPU | PMU-mode time |
| --- | ---: | ---: | ---: |
| 1/8, 1,000 | -18.93% | -11.43% | -14.22% |
| 1/8, 10,000 | -19.57% | -19.85% | -18.65% |
| 4/8 default, 1,000 | -21.24% | -17.39% | -14.36% |
| 4/8 pinned, 1,000 | -25.00% | -37.94% | -18.80% |
| 4/8 default, 10,000 | -33.36% | -30.08% | +453.41% |
| 4/8 pinned, 10,000 | -51.97% | -28.29% | +703.44% |
| 4/64 default, 1,000 | -25.72% | -22.36% | -29.30% |
| 4/64 pinned, 1,000 | -9.40% | -19.12% | +33.63% |
| 4/64 default, 10,000 | -23.72% | -25.10% | -13.82% |
| 4/64 pinned, 10,000 | -27.14% | -25.89% | -17.86% |

Collection reverses some effects. In the long pinned 4/8 control, median process
voluntary switches are 3,242.5 / 3,044 for bare A/B versus 6,096.5 / 2,109,630
with counters. Median process CPU is 14.69 / 10.53 seconds bare versus 15.14 /
93.06 seconds with counters. This is mode-sensitive execution, not an established
PMU/kernel cause. It warrants re-evaluation, not dismissal of a bare failure.

## Capacity and buffer screen: decisive stop

All these cases use four carriers, 64 live tasks and 1,000 values per producer.
Configured task capacity and channel buffer capacity are distinct dimensions.

| Buffer / task capacity / placement | Bare time | Bare process CPU |
| --- | ---: | ---: |
| 1 / 1,024 / default | **+30.41%** | -22.01% |
| 1 / 1,024 / pinned | -6.63% | -18.84% |
| 1 / 65,536 / default | -70.43% | -70.16% |
| 1 / 65,536 / pinned | -72.33% | -68.78% |
| 64 / 64 / default | -26.45% | -29.68% |
| 64 / 64 / pinned | -23.01% | -22.56% |
| 1,024 / 64 / default | -22.40% | -29.99% |
| 1,024 / 64 / pinned | -17.57% | -15.55% |

The losing cell's time ratios are **1.4373, 1.2107, 0.6559, 1.3975**. Three exceed
1.10 and the median exceeds 1.10: the declared early stop. The paired geometric
mean is 1.1238 with a wide four-pair bootstrap interval. This conservative decision
does not prove that all production workloads regress by 30.41%. Lower whole-
process CPU does not waive worse median-round time; they summarize different
parts of execution.

Voluntary switches in that cell are 10,113–12,032 for A versus 9,292–9,798 for B.
This does not support simply blaming extra native waiting. Specific owner-sleep
and preemption conditions were not sampled in these default processes.

At capacity 65,536, B takes about 2.7–3.3 us/value versus A's 9.5–12.2 us/value.
The scan remains in both versions; this is not capacity-independent maintenance.
No broader confirmation or polling adjustment rescues the failed cell.

## Endpoint tails remain unqualified

The existing sampled fixture records full send/receive calls with Instant overhead
and recording inside elapsed time. Both directions and all logical streams are
checked; warmup observations are excluded. These are closed-loop distributions.

| Shape | Send p99.9 paired ratio | Receive p99.9 paired ratio |
| --- | ---: | ---: |
| 1/8 | 0.988 | 0.988 |
| 4/8 default | 0.617 | 0.808 |
| 4/8 pinned | 0.842 | 1.080 |
| 4/64 default | 0.737 | 0.776 |
| 4/64 pinned | 0.128 | 0.128 |

Merged improvement must not hide the small pinned receive loss. Every p99.99 and
worst-stream observation is preserved. Candidate 64-task p99.99 still reaches
roughly 42–49 ms, with maxima in tens of milliseconds. This is not qualified
low-tail or fairness behavior. The unaffected, quiet/burst and twelve-pair
confirmation panels remain explicitly unrun after the stop.

## Qualification and preservation

An ordered retained-source negative fails while the publisher is held: metadata
cannot be observed. The candidate regression passes. Deliberately classifying
a rearm as a first enqueue fails its accounting assertion before restoration,
not merely compilation.

The candidate passes 22 native channel tests, 24 with profiling, strict workspace
all-feature clippy, twelve channel/model tests and seventeen retained publication
composition tests. Coverage includes selected cancellation, FIFO bounds, value/
input recovery, close, destruction, unwind, late offers after retirement, and
reuse after direct/inherited cancellation or timeout. Both default benchmark
builds pass 47 tests, formatting and clippy. Thirteen analysis tests check pair
balance, denominators and corrupted diagnostic/tail/idle accounting, including
helpers whose native panels remain unrun.

The channel model imports production Entry and WaitWord through a bounded
queue/value/atomic adapter. A routed-wake counter is not a native sleep model;
the separate retained publication composition has its documented Signal adapter
limitation. Neither proves the full runtime/channel program. The candidate has
no canonical, burst/idle, loaded-tail, large-soak or architecture promotion gate.
No zrail inventory change or grant was made.

The full candidate is preserved in stash
`844ec84dce1fd0696604de51eb7408fd2293a56f` and its source-keyed archive.
Restored native source is exactly
`d38dcb8627f61e49f3f95e46c270b9349c6629365060ac98e2dab6bbb00bfa94`.
Retained canonical run `run-1788731794-405597480-3338421` passes all fourteen gates
with repository state preserved, including default-native debug/release,
architecture policy and application smoke. This does not qualify the candidate.

The [durable bundle](evidence/channel-retry-349bc73a.tar.gz) contains 2,739
checksum-verified payload files, seven reconstructed source snapshots, all raw
processes, four benchmark/profiling binaries, negative controls and the retained
qualification receipt/logs. Its replay checks thirteen analysis tests and every
paired result and directional tail. Archive SHA-256:
`bc8f2c4d583fc76678937bbf779f9aa74c8b35814615ccbe4ce75bee248ba041`.

Next: independent production mutex round-trip attribution, measuring actual
owner/recipient conditions and work before useful dispatch without another
ownership representation or larger spin budget. Channel retry removal is a
demonstrated mechanism, not a shipped win. Capacity scans, readiness acceptance,
bounded scheduler service, current ARM64/sanitizer evidence, large live/mixed
populations, loaded tails and historical release findings remain open. No HTTP
or execution-engine work is included.
