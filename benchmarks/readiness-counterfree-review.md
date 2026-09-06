# Existing readiness candidate: counter-free promotion remains held

The existing incremental implementation survives the gross-regression screen,
but the single predeclared confirmation does **not establish protected non-
regression**. Do not promote it or keep sampling until it passes. This is a held
optimization, not a demonstrated readiness correctness defect or a new reactor.

Retained base: `fe6eb0a`, source `e344a60e`. Candidate source:
`68051406076f4f0e9700196728123e8945bd15b8380a06c167363f6dec84aa54`.
All eleven restored readiness files exactly match `f127ed1`; the archive verifier
checks their bytes against that commit. Stacks, polling, affinity, fairness,
publication completion interest and mutex/channel ownership remain unchanged.

## Collection-mode screen

All 128 planned processes complete: four A/B pairs per workload, placement and
collection mode. One-carrier TCP uses CPU 7; four-carrier TCP and tight/spare
lifecycle use CPUs 0-3. Individually pinned and default placement stay separate.
A/B and bare/counter order are balanced; endpoint build guards are clear.

| Screen case | Median paired bare time change | Median paired counter-mode time change |
| --- | ---: | ---: |
| TCP / 1 / default | -4.94% | -4.02% |
| TCP / 1 / pinned | +0.85% | -9.12% |
| TCP / 4 / default | +13.75% | +2.73% |
| TCP / 4 / pinned | +1.07% | +2.00% |
| Lifecycle / tight / default | +0.08% | +2.83% |
| Lifecycle / tight / pinned | -0.69% | -3.22% |
| Lifecycle / spare / default | +0.14% | -0.70% |
| Lifecycle / spare / pinned | -0.80% | +0.87% |

The default four-carrier TCP loss exceeds 10% in only two pairs, not the three
required by the predeclared early stop. No cell crosses that stop. The subsequent
confirmation is a separate planned panel, not replacement samples.

## Twelve-pair counter-free confirmation

All 192 new processes complete: twelve A/B pairs for each protected cell. Timing
means throughput-derived whole-round ns/op, not individual operation latency.
The table gives paired ratios; the upper bound is the predeclared one-sided 95%
bootstrap bound on the paired geometric mean. CPU includes setup/teardown.

| Case | Paired median time change | Time upper bound | Paired median CPU change | CPU upper bound |
| --- | ---: | ---: | ---: | ---: |
| TCP / 1 / default | -0.60% | +7.30% | -0.98% | +2.45% |
| TCP / 1 / pinned | +1.75% | +13.48% | +0.45% | +6.30% |
| TCP / 4 / default | +5.07% | +12.30% | +7.49% | +12.13% |
| TCP / 4 / pinned | -0.23% | +10.75% | +1.81% | +12.45% |
| Lifecycle / tight / default | -0.42% | +1.17% | +3.89% | +9.46% |
| Lifecycle / tight / pinned | -0.11% | +26.13% | +6.67% | +15.49% |
| Lifecycle / spare / default | +0.68% | +7.94% | -3.71% | +3.90% |
| Lifecycle / spare / pinned | -0.64% | +16.96% | +1.60% | +11.52% |

Default tight lifecycle timing passes the 5% margin; its CPU does not establish
non-regression. One-carrier TCP p99.9/p99.99 pass the 10% margin, while four-carrier
tails and worst-task p99.9 remain inconclusive. Pinned lifecycle is visibly
multimodal in both arms; every process remains included. These bounds do not
establish a stable regression beyond the margins either. The correct result is
held, not passed and not a newly proven universal slowdown.

The first decision table used incorrect worst-task field prefixes and omitted
those checks. The raw analysis already contained all fairness ratios. A negative
analysis test and complete decision table now include `task_median_max_ns`,
`task_p99_9_max_ns` and `task_worst_ns` at the same predeclared margin. Both tables
are retained; no run changes and the held decision is unchanged.

## Proof and retention

All 23 current-source native debug readiness tests pass, including installation/
cancellation boundaries, stale events, command/event bounds and the production
metadata model (247/1,400 states; 1,541/9,941 edges). Both default benchmark builds
pass 47 tests, formatting and clippy; baseline binary identity matches the prior
quiet-window control. Seven analysis tests pass. Full candidate canonical,
optimized runtime, offered-load, footprint and architecture/sanitizer qualification
are not claimed by this performance re-evaluation.

No population run follows the protected hold. The historical large-population
gain remains evidence in its original counter mode, not a new counter-free
scaling claim. The candidate and proof stay available at stash
`2c57469096150f28b4ae2bd76076792b859535b0`; the retained runtime is restored.

[Durable bundle](evidence/readiness-counterfree-68051406.tar.gz): 3,007 verified
internal hashes, complete source bytes and original-candidate identity checks,
all 320 processes, qualification output, scripts and both decision tables.
SHA-256: `54a0ecf17c98173aeea712f63cf4f830724f077f1fe70137e38d4d1d61e2ac19`.

The next capacity experiment is evidence-directed: inspect/eliminate duplicate
lone-admission publication before the already-required first-mount publication,
without changing polling, admission fairness or the first-mount diagnostic bound.
Bounded pre-dispatch wake service remains a separate subsequent slice.
