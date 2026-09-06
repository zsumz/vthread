# Bounded incremental readiness: promotion held

Base: `495c931`; candidate source
`01b3303835bef9e3f4b658512c4cc8355fa4525c3d3057e119d924ee5daa485a`.
The implementation is preserved on `perf/readiness-incremental`, **not promoted to
the production perf branch**. It removes the demonstrated readiness population
scan and has a substantial scaling win. Its protected-workload results do not yet
establish non-regression. This is neither a released speedup nor a new May result.

## What changed

Readiness changes are recorded when subscriptions are admitted, cancelled or made
ready. A preallocated dirty queue holds at most one key per outstanding entry.
Installation and retirement each have queued/in-flight phases. An entry continues
to consume admission capacity until deletion is acknowledged; dropping a lease
cannot wait for command space, free its identity early, or lose an eventual delete.
Cancellation during installation is acknowledged by a queued deletion afterward.

The poll owner takes at most 64 commands or events per batch and interleaves them.
Backend mutations and wake publication happen outside shared metadata. Deletion
and capacity acknowledgement precede the ready wake, so a resumed task can
immediately register again even at full I/O capacity. Owned descriptors and exact
subscription/park identities remain protected through retirement and shutdown.
The existing level-triggered mode, failure reporting, waker and bounded shutdown
fallback remain. Shutdown may walk the population once; an unchanged ordinary
iteration examines no registration entries.

No stack, placement, checkpoint, fairness, polling, channel, mutex, public API or
HTTP changes are included. A separate unpublished lab executable measures mostly
idle socket populations using the public runtime API.

## Qualification and limits

- The old implementation fails three intended assertions: 256 unchanged-entry
  examinations instead of zero; premature reuse of removal capacity; and backend
  mutation under metadata. An earlier 1,024-entry fixture hits the descriptor
  limit before its scan assertion; that attempt is not negative proof.
- Twenty-three targeted tests cover installation/deletion pauses, cancellation,
  actual publication outside metadata, abandoned publication, exact generations,
  stale buffered events, bounded batches and shutdown during continuous churn.
- The model runs the production metadata State: 247/1,400 states and 1,541/9,941
  edges at capacities one/two, with three admissions. It separates backend work
  from acknowledgement. It models serialized metadata and one in-flight owner
  operation, not the entire 64-command batch, OS poller, wait word or signaling.
- Deliberately losing cancelled-install cleanup and removing the event bound fail
  four tests, including the production-state model and actual backend regression.
  Restoring the identical source passes. Twenty repetitions per native profile
  pass all 23 tests: 920 test executions across 40 processes, not distinct random
  schedules.
- All fourteen canonical gates pass in `run-1788676633-358052789-2770840` with
  preserved repository state. Default debug/release each pass 554 runtime tests;
  all-features passes 583. One manual borrowed-maintenance probe remains ignored.
  Workspace stack/model/lab/reexport tests, doctests and application smoke pass.

The architecture lock refresh changes analyzed inventory only: one extra lab
target and thirteen Rust files, without dependency, permission or policy changes.
No long mixed-lifetime soak, large-population release burn-in, native ARM64 or
sanitizer qualification is claimed for this candidate. The six loaded findings
remain separately unresolved; a passing canonical run does not classify them.

## Intended scaling workload

Each row contains four independent AB/BA/BA/AB process pairs. Default features,
four carriers and normal placement are unchanged. The descriptor limit is raised
only for each benchmark process to 32,768. Idle subscriptions are installed before
the exchange timer, then held through active traffic. Values and final cleanup
are checked. No snapshots or handoff timing run inside the exchange loop.

| Idle registrations | Active pairs | Exchanges per pair | A ns/exchange | B ns/exchange | Process cycles | Process instructions |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 10,000 | 127,211 | 123,311 | -7.08% | +0.05% |
| 0 | 4 | 10,000 | 2,651 | 2,711 | +3.16% | +1.06% |
| 0 | 4 | 100,000 | 3,182 | 2,929 | +1.05% | -0.13% |
| 1,024 | 1 | 1,000 | 186,407 | 99,639 | -16.76% | -52.32% |
| 1,024 | 1 | 10,000 | 210,799 | 130,708 | -49.17% | -81.53% |
| 4,096 | 1 | 1,000 | 947,008 | 106,342 | -44.45% | -66.71% |
| 4,096 | 1 | 10,000 | 967,373 | 130,968 | -82.38% | -91.09% |

The final row's cycle changes range from -81.44% to -83.06% across pairs. Its
roughly 20,000 parks per process are essentially equal across engines. The large
gain is not obtained by replacing suspension with retries. Four-active-pair
controls often use the existing runnable retry path; their park counts and noisy
process samples remain in the raw results.

Ns/exchange is throughput-derived, not individual latency. Whole-process counters
include idle creation and cleanup, unlike the exchange interval. Larger work
counts establish that setup savings alone do not explain the scaling result.
Population A includes a mechanical extraction of the old reconciliation helper
for negative tests; only its counters are test-only. Its code generation is not
claimed identical to pristine HEAD. Source manifests and old files preserve that
distinction. Protected A below is the retained pre-readiness runtime.

## Why it is not promoted

The protected panel uses default uninstrumented binaries, identical A/B profiles
and existing workload implementations. One-carrier cases use CPU 7, four-carrier
cases CPUs 0-3. Existing carrier pinning remains enabled where recorded; burst
placement remains normal. No own build or correctness test overlaps these runs.

| Protected case | A ns/op | B ns/op | Process cycles | Process instructions |
| --- | ---: | ---: | ---: | ---: |
| TCP / 1 carrier, first group | 132,527 | 137,746 | +3.83% | +3.14% |
| TCP / 4 carriers, first group | 47,763 | 47,663 | +2.60% | -1.65% |
| TCP / 1 carrier, confirmation | 127,833 | 149,804 | +3.86% | +2.63% |
| TCP / 4 carriers, confirmation | 49,954 | 48,133 | +3.96% | -1.51% |
| Lifecycle / 4 / 1,000 | 422.97 | 422.55 | +0.49% | +1.23% |
| Lifecycle / 4 / 1,000, capacity 65,536 | 1,626.19 | 1,600.65 | +11.20% | -1.53% |
| Park / 4 / 64 | 124.17 | 133.94 | +1.88% | +0.42% |
| Mutex / 4 / 64 | 557.21 | 490.60 | -24.72% | -11.24% |
| Yield / 4 / 64 | 15.61 | 15.86 | +1.74% | +0.03% |
| Shared channel / 4 / 8 | 1,382.24 | 1,836.49 | +1.07% | +1.39% |
| Wake / 4 / 64 | 97.02 | 81.41 | -10.20% | -1.59% |

Higher and lower medians are not automatically causal findings. Spare lifecycle
cycles range from -7.37% to +27.79% across pairs. Channel time has very large paired
variation despite nearly flat cycles. The mutex and wake rows are not new claimed
wins in those unchanged primitives.

Four-carrier TCP p99.9 rises 7.14% in the first group but only 0.36% in confirmation,
with mixed paired effects. Worst-task tails likewise do not establish a stable
regression or improvement. Absolute TCP tails are already tens of milliseconds
in both arms; these runs do not identify their cause or qualify loaded tails.

The existing burst/quiet diagnostic was rebuilt against both exact runtimes with
identical dependency locks. At 100-us gaps, CPU medians rise 12.92% then 3.38%,
while cycle medians fall about 3% and 2.45%. The confirmation's paired CPU effects
range from -16.33% to +19.60%. One-ms and quiet controls show no persistent penalty.
No source, backoff or polling adjustment follows these mixed observations.

Taken together, these are insufficient evidence of the required protected-workload
non-regression. Preserve the algorithm/proof and hold promotion; do not convert
the population win into a universal speedup, dismiss unfavorable samples, or claim
that every noisy higher median is an established runtime defect. Next readiness
work needs attribution or stronger controlled evidence for these protected cases,
not another polling search. Capacity-independent kernel observation remains open.

## Durable evidence

[Source-keyed bundle](evidence/readiness-incremental-01b33038.tar.gz), SHA-256:
`aa4241bdf0dba95cae5ac59b5f13dafd882c984c90d0d3bc476bc36d15d7df6c`.
All 1,893 internal hashes verify; the complete candidate patch applies to its base.
It contains failed/passing tests, model limits, 920 repeated executions, canonical
receipt, source/binary identities, replay/analysis scripts and 192 completed
counter processes. Four guard-stopped groups have no invocation; all completed
processes are retained. The unpaired population smoke is separate. Executables
are identified by hashes, not embedded as release artifacts.

Endpoint build guards do not prove continuous physical-host isolation. These are
Linux x86-64 VM measurements with recorded compiler, kernel, CPU masks and process
commands. Burst waves are closed-loop, not offered-load qualification. Full
footprint, offered-load, architecture/sanitizer and frozen-source May evidence
remain open. The next independent slice is the missing default-build channel
attribution pass, before any further channel implementation.
