# Resident wait fault construction: useful work removed, promotion held

The single runtime candidate is **held**, not a retained performance improvement.
It removes demonstrated, unnecessary global atomic work from successful resident
wait validation and improves the local and default-placement mutex screens.
Protected TCP elapsed time, CPU and tails do not establish non-regression.
No polling, ownership, stack, affinity or publication-protocol variant follows.

## Exact sources and the change

- Retained native source: `325396ac5f820b4714ccc439d994cb947e2fbb977ceb4a2a4be19a8491018cbe`.
- Candidate native source: `c2289e693ebad73f68c58fb72752a1113260bec36e645ad5a60fe33883c36728`.
- Baseline binary: `6b1942226797bd4170c886fdfa5eeb698c3383a824d92f21e5a1610802b8a316`.
- Candidate binary: `18c01827e605a0fc88fd0fc1bdb8ffb56b403e314eed7461c7a10ac37f50efc0`.

`Execution::synchronization_wait_for` used `.ok_or(Error::fault(...))`.
`ok_or` eagerly constructs its argument even when the lookup succeeds.
`RuntimeFault::new` obtains a process-global incident ID with an atomic
`fetch_update`. An ordinary resident park and its successful wake-consumption pass
therefore construct and discard two faults, including two global RMW operations.
Deferred owner revisits can do additional work through the same helper.

The candidate changes only that expression to `.ok_or_else(|| Error::fault(...))`.
It adds no allocation, queue, counter, unsafe code or synchronization primitive.
Actual failures still create distinct typed Scheduler faults. Incident numbering
changes because successful operations stop consuming IDs; gap-free numbering was
never the contract. Exact wait identity/generation validation, both cancellation
checkpoints, completion-interest handling, retirement and mutex ownership remain
unchanged. The production wait-word, wake queue and modeled protocol files are
byte-for-byte unchanged.

This is not merely source-level speculation. In baseline `process_wake`, the
locked CAS on `RuntimeFault::new::NEXT` is at `0x10e91e`, before the successful
pointer test at `0x10e969`. In baseline `tick`, it is at `0x11146e`, before the
validity branch at `0x1114ba`. Both successful paths also discard the constructed
error. In the candidate, successful identities branch around fault creation;
the publication completion-interest CAS remains. The bundle preserves binary
identities and disassembly extracts, not an assumed instruction-count forecast.

An earlier resident-load-fusion patch was prepared but **never built or benchmarked**.
It was restored before this narrower candidate. Its patch is retained as
`fusion-unmeasured.patch`; it is neither an accepted change nor a failed timing sample.

## Regressions and qualification

The existing-entrypoint publication regressions first passed on the old production
code: 45 tests selected by the publication filter. The new zero-fault regression
then failed on that code with **2 created faults versus 0 expected**. This is the
negative control, not an unexplained runtime failure. The lazy candidate passes
all seven context-pending tests, including distinct typed faults for invalid
identity/generation, missing registration, exact panic context, generation reuse,
and active/selected/retired publication states. Shared registrations retain their
separate stale-result contract.

Source policy and `zrail check` pass without changing the architecture lock or
granting permissions. Six replay-parser tests include five negative controls for
missing rounds, wrong workload, capacity, operation count and percentile values.
Canonical `zcheck run check` passes **all 14 required gates** on the candidate,
including default-native debug (80.9 s), native release (257.8 s) and all-feature
workspace tests (115.5 s). The complete run takes 474.8 s and preserves the source.
The bundle's `canonical/receipt.json` and task logs retain run
`run-1788738833-604743433-3387841`, not only a summarized green result.

This is Linux x86-64 qualification, not current-source ARM64, alternate-stack
sanitizer support, large-population stress or release completion.

## Counter-free screens

160 baseline/candidate processes completed: 20 cells, four independent A/B pairs
each, with AB/BA/AB/BA process order. Every planned outcome is retained. An additional
12 unchanged-binary counter-free controls and five collected diagnostics completed.
The latter are two cycle-sampled and three scheduler-traced processes, not timing
acceptance runs. All benchmark builds use default native features; no clocked
handoff profiler is enabled in these comparisons.

Single-carrier mutex still yields under ownership. Multicarrier mutex still runs
32 `black_box` operations in the critical section. Process mask is CPU 0 for the
local screen and CPUs 0-3 for multicarrier candidate comparisons. Individually
pinned and default-placement cases remain separate. Each process includes one
warm-up; process CPU includes setup, warm-up and teardown. Reported ns/op is
whole-round elapsed divided by work, not individual acquisition latency.

The local screen predeclared a minimum 1% paired-median elapsed improvement,
three of four improved pairs, and process CPU within 5%. It passed: elapsed
**-1.54%**, CPU **-2.20%**, with all four elapsed ratios below one. The subsequent
protection margin was 5%; a gross early stop required a >10% paired median and
three of four >10% losses. No gross stop fired, but that is not acceptance.

Tables show changes in the median of paired candidate/baseline ratios. Negative
is less time/CPU. Raw process medians and every ratio are preserved in the bundle.

| Initial screen | Calls/task; measured rounds | Elapsed | Process CPU |
| --- | --- | ---: | ---: |
| Mutex, 1 carrier / 8 tasks | 10k; 101 | -1.54% | -2.20% |
| Mutex, 4 / 8, default | 1k; 101 | -11.63% | +1.09% |
| Mutex, 4 / 8, default | 10k; 21 | -25.04% | -7.36% |
| Mutex, 4 / 64, default | 10k; 21 | -12.14% | -3.98% |
| Mutex, 4 / 8, pinned | 1k; 101 | -9.27% | -0.46% |
| Mutex, 4 / 8, pinned | 10k; 21 | -3.71% | -15.16% |
| Mutex, 4 / 64, pinned | 10k; 21 | -5.42% | -7.95% |
| Shared channel, 4 / 8, default | 1k; 21 | -5.63% | -8.91% |
| Shared channel, 4 / 64, task capacity 1,024 | 1k; 21 | +2.00% | +9.71% |
| Park, 4 / 64, pinned | 10k; 21 | -8.62% | -1.33% |
| Wake, 4 / 64, pinned | 1k; 21 | +2.08% | +8.51% |
| Yield, 4 / 64, pinned | 10k; 21 | -1.74% | -1.89% |
| Spawn, 4 carriers / 1k tasks, tight | 101 rounds | -62.09% | -33.43% |
| Spawn, 4 / 1k, task capacity 65,536 | 101 rounds | +0.37% | +7.44% |

The large unrelated lifecycle difference is an observed process result, **not**
attribution of a lifecycle improvement to lazy fault construction. The initial
long pinned eight-task mutex ratios range from 0.841 to 1.903 despite the lower
paired median. Four pairs (eight processes) do not establish precise non-inferiority bounds.

One bounded confirmation extended the ambiguous CPU/placement cells and added
10k-task lifecycle and the protected TCP screen. There was no code change.

| Confirmation, four carriers | Measured rounds | Elapsed | Process CPU |
| --- | ---: | ---: | ---: |
| Mutex, 8 tasks, 10k calls/task, pinned | 101 | -15.87% | -13.22% |
| Channel, 64 tasks, task capacity 1,024; channel capacity 1 | 101 | -2.33% | -1.83% |
| Wake, 64 tasks, pinned | 101 | -0.78% | +0.09% |
| Spawn, 1k tasks, capacity 65,536 | 501 | +0.93% | +2.47% |
| Spawn, 10k tasks, tight | 101 | -6.97% | -11.65% |
| TCP, 8 tasks, 1k exchanges/task, pinned | 11 | **+12.14%** | **+9.94%** |

Initial wake p99.9/p99.99 ratios were 1.094/1.241; confirmation ratios were
1.012/1.069. TCP p99.9's paired-median ratio was **2.020**, while p99.99 was 0.964
and maximum 1.000. These are closed-loop endpoint distributions, not offered-load
SLOs. The benchmark exports distribution summaries, not every endpoint timestamp.
TCP elapsed ratios span 0.873-1.413: this is insufficient to classify the exact
causal regression, but clearly insufficient to accept the patch under the margin.

No cycle-per-operation candidate comparison, burst/idle acceptance or offered-load
candidate panel followed this stop. Optimized removal of the RMWs is code evidence,
not a substitute for those measurements. There is no new May comparison.

## What the scheduler/host traces add

In the baseline 64-task cycle sample, 65.85% of leaf weight is in the inlined
carrier wrapper. Annotation places most wrapper samples immediately after the
idle loop's `pause`; PMU skid prevents treating that instruction's weight as an
exact per-stage latency. `WaitHub::pop` has 7.87% leaf weight, `process_wake` 3.60%,
`tick` 3.57%, and mutex release 1.96%. This did not justify another polling policy.

A targeted trace supplies a concrete host-availability witness. At approximately
monotonic time 889308.615885, carriers on CPUs 0-2 switch out in `S` and remain off
CPU for about 49.85-49.90 ms. Their wake-to-run delays are only 11-22 us. CPU 3's
carrier remains guest-scheduled through a 99.917 ms interval encompassing those
sleeps. Nearby `/proc/stat` samples account for about 30 ms of CPU 3 user time and
50 ms of steal across an encompassing 80.6 ms observation window (`CLK_TCK=100`).

Steal accounting is batched: a five-tick increment observed between 20 ms samples
does not locate 50 ms of theft inside that particular sample interval. Across the
longer observation window, CPU 3 reports 35.68% steal. Guest scheduler residence
is not equivalent to actual execution while a virtual CPU is unavailable. This
trace does not identify the selected mutex generation, blocking syscall or the
hypervisor's underlying cause; it is not a complete handoff-stage attribution.

An eight-process, balanced **same-binary** CPU-mask control compares 0-3 with 4-7,
each individually pinned. It finds no material benefit from simply choosing the
other CPU set. A quiet process list is not proof of an exclusive physical host.
No slow outcome was discarded, and no older rejected candidate is retrospectively
qualified by this environmental finding.

## Stop decision and recovery

Hold this exact, minimally changed candidate for independent protected confirmation,
preferably on dedicated CPUs. Do not combine it with load fusion, another channel
layout, increased polling or mutex ownership changes. The pending question about
available dedicated hardware remains separate from the runtime's correctness.

The candidate and its tests are preserved as stash
`2802371388d8a6108986b0898c25ae2f0d9fb766` and as the bundle's
`lazy-fault-build.patch`, based on commit
`bc545330abc8bab3c18622f9b9c001daef83fee3`. The restored production source hash
is exactly the retained `325396ac...` identity above.

[Durable evidence bundle](evidence/resident-wait-fault-c2289e69.tar.gz), archive SHA-256:

```text
78b4f14838d5f16f25a6dd24f327ab61ad8a996d23b1332cd12103b6161e1d83
```

The archive includes both executables, source archives, complete candidate and
unmeasured-fusion patches, exact commands/configuration, raw outcomes, scheduler
traces, CPU observations, disassembly, qualification logs and replay tools.
`identity-supplement.tar.gz` supplies unchanged compiler configuration/workflows
from the base commit; together with each source archive it reproduces the full
recorded source digest. All **495 payload hashes** verify after fresh extraction.
The standalone analysis exactly reproduces the recorded remote/protected/
confirmation ratios; the local screen and unchanged-binary controls retain their
separate plans, raw results and decisions.

From an extracted bundle, without running either executable:

```sh
sha256sum --check SHA256SUMS
PYTHONDONTWRITEBYTECODE=1 python3 tools/vthread-handoff-identities.py .
PYTHONDONTWRITEBYTECODE=1 python3 tools/vthread-handoff-analyze.py .
PYTHONDONTWRITEBYTECODE=1 python3 tools/vthread-handoff-analyze-test.py tools/vthread-handoff-analyze.py
```

The other scripts preserve the original host-specific execution paths for
provenance; they are not automatic permission to rerun or overwrite evidence.

Capacity-independent maintenance, incremental-readiness promotion, exact sampled
handoff-stage association, offered-load tails and the other release-proof items
remain open. The retained perf branch receives evidence only from this slice.
