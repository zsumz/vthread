# Mutex controls: partial measured panel, no runtime change

Historical partial attempt. The later
[complete quiet-window controls](mutex-mechanism-quiet-review.md) use new,
independent invocations and do not pool or replace the observations below.

The [qualified fixture](mutex-mechanism-review.md) was resumed on a clean detached
`80b3ba1` worktree, verifying source `f0ce5246` and both previously qualified
executable hashes. The perf branch's subsequent changes are test/policy repairs,
not production mutex, scheduling or polling changes.

**Twelve default processes completed; eleven have clear endpoint build guards.**
The twelfth is excluded after its guard observed another cargo process. Six
planned default processes did not run. The diagnostic panel's first guard also
stopped it before execution: zero diagnostic processes ran. The full analyzer
rejects this incomplete/excluded input; no complete comparison is manufactured.

## What the partial controls show

Each row below has **two** independent 10,000-handoff processes, not the planned
three. Values are ranges of process-level quantiles, not pooled observations.

| Forced recipient state | p50 | p99.9 | Largest individual sample |
| --- | --- | --- | --- |
| Same owner | 190–191 ns | 250–271 ns | 136,655 ns |
| Remote, active keeper | 321–340 ns | 501 ns | 41,402 ns |
| Remote, observed asleep | 10,466–10,486 ns | 45,459–54,611 ns | 19,224,784 ns |

The shorter 1,000-handoff controls are also preserved, including an observed-sleep
maximum of 19,244,254 ns. No tail is discarded because the median looks good.
The guard exclusion is conservative evidence of an observed build, not proof of
what delayed any individual sample.

Owner IDs, measured-task TIDs and affinity are verified. Sleep is observed before
release, not atomically established at publication. Command parks contribute to
the recipient's park count. This is a forced-state, one-successor, closed-loop
fixture—not the production contended-mutex workload, offered load or fairness.
Its state frequencies cannot explain May's lead without additional attribution.

Whole-process cycles include startup, command handshakes, source/keeper yields,
procfs observation, shutdown and raw-sample output. For the longer controls,
cycles/handoff range from 19,318–22,852 local, 40,806–52,901 remote-active and
384,552–392,898 after observed sleep. These are **not isolated mutex costs** or
the cycles spent inside the timestamped interval. The two work counts make that
interpretation boundary visible; no ownership-slot optimization is justified.

## Evidence and next work

The [152-file verified bundle](evidence/mutex-mechanism-partial-f0ce5246.tar.gz)
preserves all twelve raw processes/counters, before/after guards, commands,
frozen source/binary identities, the complete fixture patch/receipt, four passing
parser tests, the failed complete-panel analysis and explicitly partial inventory.
The executed binaries remain local. Archive SHA-256:
`6f36e4a020233e7179eceb285e35f97ed2e792978cf5cddc61ac2e473f402755`.

No runtime candidate is promoted and no new May result is claimed. The complete
balanced default/diagnostic panel remains open. Work returns to the independent
ordered timer/join/inbox findings; direct mutex ownership and polling stay frozen.
