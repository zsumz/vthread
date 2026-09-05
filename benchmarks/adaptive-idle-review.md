# Adaptive polling: first screen remains below acceptance

This is a separate experiment after [fixed long polling](channel-tail-review.md)
failed the burst-CPU gate. It retains the channel eligibility predicate but earns
4,096 probes only after 16 consecutive complete idle intervals of at most 50 us.
One longer interval restores the original 640 probes. Timing includes polling
and native sleeping; it is excluded for single-carrier, timer, admission-only and
already-visible-work waits. The added state is one carrier-owned byte, with no
new shared atomics, allocations or changes to wake/ownership ordering.

Candidate source SHA-256:
`af71b5774299e3fcb07c649ac73072d706f619b15092dfa43e70fbd35fa87ec2`.
Baseline is qualified checkpoint `070c11f`, source `30bb4869`. Immutable source
patches, compiler/library/binary hashes and exact commands are in the evidence
bundle. A rebuild after refreshing only zrail analysis counts is byte-identical.

## Correctness scope

The fixed policy fails an ordered native first-park budget regression, captured
after cleanup: 4,096 probes rather than 640. The adaptive candidate passes three
idle tests, including all 65,536 sixteen-step short/long histories, saturation,
the real parked-task budget boundary, and timer/single-carrier/no-park exclusions.
All 18 channel tests also pass. Initial syntax and test-configuration failures are
preserved with their subsequent repairs.

The architecture check passes and its refresh grants no new permissions or debt.
These are targeted tests, not final canonical, long stress, full concurrency
composition or release qualification. A private pacing state machine does not
require weakening or replacing the existing wait/sleep atomics.

## Four balanced pairs per case

The screen contains 40 measured processes across five cases. A further two-run
pair is retained separately because a host build appeared in its final process
observation; the complete pair was rerun. Own builds/tests never overlap timing.
Accepted endpoint observations catch no guarded build process; they cannot prove
exclusive physical-host access or absence of activity between observations.

| Case | Baseline | Adaptive | Change |
| --- | ---: | ---: | ---: |
| Shared MPMC, 4 carriers / 8 tasks, capacity 1, ns/value | 1,284.54 | 1,059.73 | -17.50% elapsed; -36.53% cycles |
| Shared MPMC, 4 / 64, capacity 1, ns/value | 1,421.30 | 938.24 | -33.99% elapsed; -31.86% cycles |
| Sampled receive p99.9, 4 / 8, us | 31.52 | 46.98 | Worse |
| Sampled send p99.9, 4 / 8, us | 35.36 | 46.89 | Worse |
| Burst CPU, 4 / 8, 1 ms gaps, ms | 451.64 | 465.26 | +3.01% |
| Burst CPU, 4 / 8, 100 us gaps, ms | 1,282.85 | 1,617.73 | +26.10% |

Each value is the median of four process results. Endpoint medians and throughput
improve, but p99.9 does not clear the gate. Individual tail results vary widely;
no cause is assigned without stage/OS evidence. The 100-us burst cycles rise only
5.28% and instructions 0.77%, versus 26.10% CPU time. Do not equate those measures
or attribute the full CPU increase to additional instructions.

Throughput is whole-round elapsed time per transferred value. Sampled endpoint
latencies include their clock overhead and are closed-loop, not an offered-load
or fairness bound. CPU/cycle counters cover the complete process, including
warm-up, setup, validation, distribution processing and teardown as applicable.
The shared-channel cases pin carriers; burst placement is normal within the
process CPU mask. All comparisons use identical workloads in both arms.

## Decision

Shelved. It removes most of the fixed policy's roughly 2.6x burst CPU cost and
retains substantial throughput gains, but fails the combined CPU/tail gate. A
green correctness suite would not make those performance failures disappear.
No adaptive policy is shipped and no May comparison is rerun.

After shelving the prototype, source `30bb4869` is restored exactly. All 11
canonical gates pass on that baseline before this evidence-only checkpoint; its
full receipt and logs are included separately in the bundle. That qualification
does not cover or accept the adaptive runtime candidate.

Next investigate measured complete-idle durations, useful poll hits, native waits,
and resume/publication stages before proposing another policy. Keep those
diagnostics out of headline binaries. The channel's safe out-of-lock publication,
capacity-independent maintenance and broader release-evidence work remain open.
