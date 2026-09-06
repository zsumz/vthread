# Capacity scans and admission-only polling: rejected

Base: `7f6e9cc652f30d1bd4db3d664a498445e2bc5766`, including the qualified paused-
publisher progress repair. The isolated experiment branch is
`perf/capacity-admission-idle`. No runtime change from this experiment is retained.

## Four controlled arms

| Arm | Carrier capacity scan | Admission-only probes | Parked handoff probes |
| --- | --- | ---: | ---: |
| A | Retained | 640 | 640 |
| B | Removed | 640 | 640 |
| C | Retained | 0 | 640 |
| D | Removed | 0 | 640 |

Scan removal moves remote depth to explicit snapshot observation. Carrier
publication retains local wakes and owner-deferred notices; the observer adds
fresh remote depth rather than replacing those contributions. No per-wake counter,
new mailbox, admission batching/quota change or signal/sleep reordering is added.
Completion flushing and cached visible ingress are preserved.

Test-only counters check actual depth observations and probe visits. Three snapshot
assertions fail retained A: publication scans twice, local depth is overwritten,
and a consumed-but-deferred wake disappears from the reported pending depth. Fresh
remote observation already passes. The admission-only probe check fails A while
the parked-handoff check passes. The initial idle fixture had an invalid default
stack-cache capacity; its configuration failure is retained separately, not counted
as a policy negative control.

Combined D passes 66 kernel tests (one manual performance probe ignored) and three
cached-ingress tests, including publication deferral, ready/admission fairness and
the new snapshot/probe cases. This targeted evidence is not canonical qualification
or performance acceptance of an optional candidate.

## Tight-capacity screen and stop decision

Default native release binaries, no handoff clocks/profiling. Four independent
processes per arm rotate through ABDC, BCAD, CDBA and DACB; each arm occupies every
position once. Four carriers share process CPUs 0-3, normal placement, 1,000 live
tasks/capacity and 501 measured lifecycle rounds. Perf totals include warm-up,
setup and teardown; ns/task is throughput-derived from the measured rounds.

| Arm | ns/task | Process cycles vs A | Process CPU time vs A | Context switches |
| --- | ---: | ---: | ---: | ---: |
| A | 420.73 | baseline | baseline | 4,145 |
| B | 479.10 | +20.19% | +16.37% | 3,823 |
| C | 687.05 | +7.25% | +43.51% | 25,325.5 |
| D | 968.22 | +7.37% | +94.27% | 49,953.5 |

Values are medians of four process samples. Skipping admission-only probes fails
the protected tight-capacity workload: D is about 2.3x worse in ns/task and has
about 12x the context switches. Lower instruction counts do not compensate for
these losses. Switch counts support investigating native-wait costs but are not
exact syscall attribution; elapsed time, CPU time and PMU cycles are kept distinct.

The next 10,000-task group stopped after its third invocation when the endpoint
guard caught an unrelated host build. That incomplete group is excluded. No spare-
capacity, synchronization, burst or tail acceptance panel is inferred from it.
After the complete 1,000-task failure, the zero-probe candidate is rejected.

## One fixed-budget follow-up, then stop

Arm E uses scan-free publication with 32 admission-only probes, still 640 with
parked work. Notification is ordered after wait arming, so tests observe the exact
32/640 budgets. The 32 assertion fails the zero-probe implementation; E passes all
66 kernel tests. This is the single permitted fixed-budget follow-up, not a search.

Four clean A/E pairs in AE/EA/EA/AE order at the same 1,000-task work count report
433.99 to 802.88 ns/task (+85.00%), +10.32% process cycles, +93.35% CPU time and
4,146.5 to 34,615 context switches. E also fails the protected workload and is
rejected. No further probe budgets, parked-polling increases or adaptive variants
were tried. No full canonical, spare-capacity or tail qualification was spent on
either losing optional candidate.

## Preservation and next independent target

All 27 completed invocations are retained: 16 complete factorial controls, three
excluded partial/overlapped 10k controls, and eight complete fixed-budget controls.
Endpoint observations are not proof of uninterrupted dedicated-host exclusivity.
Per-arm source/executable identities, exact commands, raw counters, negative and
passing tests, patches and analysis are preserved in the evidence bundle.

The [source-keyed archive](evidence/capacity-admission-4aaf5b0a.tar.gz) has SHA-256
`8426b17dba434dd5bfafd27a52f2ff7206abefa8408190d6f901ce89f2ddaa09`.
Extraction and all internal hashes were verified. It contains evidence and binary
identities, not shipped executables or a complete release qualification bundle.

The full E source is also recoverable from stash
`8c2481e972030efe72938e62a7b93246832a76a8`; B/C/D patches and their original fixture
sources are independently preserved. The qualified perf branch is restored to
runtime source `90bc31261224939e4e1f15fbecd09ca29784d348ce2175dbe901a6f22541511d`.
The capacity scan and local/deferred snapshot-depth issue remain open; rejecting
the optimization does not close those findings. Incremental bounded readiness is
the next independent implementation slice. The May comparison stays historical.
