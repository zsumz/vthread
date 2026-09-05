# Channel gains fail the burst-CPU gate

The fixed parked-task polling experiment is **rejected**, despite substantial
shared-channel gains. It combines useful-direction notification predicates with
4,096 idle probes whenever a carrier owns parked tasks, versus the baseline 640.
No ownership, wake-claim ordering, carrier affinity or admission policy changes.

Baseline is the independently qualified benchmark checkpoint `070c11f`, source
`30bb48693c14080d0a289f76a23943f9d67caf1507b20e7e69b8db302dcdad9c`.
Candidate source is
`c6a2bd0a02d9d93b5fced5d98c6bb99df58c030b7100231081e23fd6470ca6cf`.
The preceding [four-way experiment](channel-handoff-review.md) isolates the
notification/polling interaction; this follow-up tests its missing acceptance gates.

## Main panel

Four balanced baseline/candidate pairs across 17 cases produce 136 independent
process runs. Values below are medians across the four process results, not
individual-operation latencies. Whole-process hardware counters include warm-up,
validation and, when enabled, latency-distribution processing.

| Untimed workload | Baseline ns/op | Candidate ns/op | Cycle change |
| --- | ---: | ---: | ---: |
| Shared MPMC, 2 carriers / 8 tasks, capacity 1 | 1,059.04 | 821.80 | -13.11% |
| Shared MPMC, 4 / 8, capacity 1 | 1,285.29 | 783.47 | -45.66% |
| Shared MPMC, 4 / 64, capacity 1 | 1,370.93 | 882.06 | -38.56% |
| Shared MPMC, 4 / 64, capacity 64 | 1,396.84 | 1,268.07 | -0.66% |
| Shared MPMC, 4 / 64, capacity 1,024 | 1,286.10 | 1,339.61 | -2.21% |
| Shared MPMC, 4 / 64, capacity 1, admission capacity 65,536 | 18,349.75 | 3,434.52 | -76.29% |
| Shared MPMC, 8 / 64, capacity 1 | 21,468.60 | 911.34 | -86.87% |
| Contended mutex, 4 / 64 | 422.76 | 387.35 | -15.52% |
| Paired bounded channel, 4 / 64 | 155.43 | 152.19 | -2.25% |
| Yield, 4 / 64 | 8.95 | 8.80 | +0.56% |

The capacity-1 shared-channel gains are material. Large buffers do not show the
same cycle reduction; capacity 1,024 has 4.16% worse elapsed throughput despite
slightly fewer cycles. Capacity-dependent scheduler scans remain in both versions.
The eight-carrier result recovers a severe baseline collapse on this eight-vCPU
guest, not proof of the same improvement on an isolated physical machine.

The [sampled endpoint control](channel-latency-review.md) separately measures
complete send/receive API calls, including clock overhead. At four carriers/eight
tasks/capacity one, median-across-process receive p99.9 improves 18.12 to 9.65 us;
send p99.9 improves 20.78 to 9.92 us. The 64-task distributions remain variable;
eight-carrier p99.99/max still reach tens of milliseconds. These are closed-loop
endpoint distributions, not an open-loop latency or starvation guarantee.

## Lifecycle discrepancy is preserved

The original 101-sample spare-capacity lifecycle control reports +13.23% cycles.
A separate 24-process, 301-sample follow-up using the same binaries does not
reproduce that increase: four-carrier normal placement -1.76%, four-carrier pinned
+1.35%, one-carrier pinned +0.36%. Both panels are retained. This is unresolved
variation, not permission to discard the first panel or claim zero regression.

## Decisive rejection: CPU between wake bursts

A separate public-API diagnostic parks every task, waits for an idle snapshot and
a settling interval, then either remains quiet or releases closed-loop wake waves.
Native sleeps and blocking acknowledgement receives stay on the native caller;
task acknowledgements use bounded nonblocking sends. Each wave verifies task and
wave identity, counts selected wakes/stored permits, and checks complete drainage.

Four balanced pairs over six cases produce another 48 process runs. These are
whole-process CPU milliseconds, including setup, observation and teardown—not an
isolated measurement of idle-loop instructions.

| Burst case | Baseline CPU ms | Candidate CPU ms | Increase |
| --- | ---: | ---: | ---: |
| 4 carriers / 8 tasks, 1 ms gaps, 2,000 waves | 441.91 | 1,155.86 | 161.56% |
| 4 / 8, 100 us gaps, 5,000 waves | 1,350.37 | 3,448.33 | 155.36% |
| 8 / 16, 1 ms gaps, 2,000 waves | 903.12 | 2,357.35 | 161.02% |

Quiet two-second controls use about 8.54/8.98 ms total CPU at tight capacity and
19.82/20.44 ms with admission capacity 65,536. The one-carrier burst control, where
polling is disabled in both versions, varies by -9.45%. Neither observation makes
the repeated roughly 2.6x multi-carrier burst CPU cost acceptable.

## Qualification and next boundary

The fixed candidate passes 18 native channel tests, its native idle-budget test,
and 51 benchmark tests. Both diagnostic arms pass bounds and real one/four-carrier
quiet/wave tests. It has no final canonical, long-soak or release acceptance.

All 208 measured processes, endpoint host observations, source patches, compiler
and binary/library hashes, commands and analysis are preserved in the source-keyed
bundle linked from [the evidence index](evidence/README.md). Executables are not
archived. Own builds/tests did not overlap timing; endpoint host samples found no
compiler/high-CPU process in the main panel. Physical-host exclusivity is unproven.
Carrier pinning is used in the main controls except normal lifecycle placement;
the burst diagnostic uses normal placement inside its process CPU mask.

The next bounded experiment must earn extended polling from observed short idle
intervals and return to the original budget after quiet gaps. Its evidence must
remain separate. The fixed-policy gains are not shipped improvements, no May
comparison was rerun, and safe out-of-lock channel publication remains open.
