# Mutex controls: complete quiet-window evidence

The [qualified fixture](mutex-mechanism-review.md) now has its complete panel:
18 default counter processes, nine diagnostic processes, and nine subsequent
counter-free long controls. **No mutex, scheduler, ownership or polling change
is promoted.** All 36 invocations pass; every endpoint build guard is clear.
The earlier partial attempts remain historical and are not pooled into this run.

## Useful handoff versus observed sleep

Headline timing uses the counter-free default executable. Each mode has three
fresh processes, 10,000 handoffs per process, with the middle mode order reversed.
Cells are medians of process-level quantiles; maxima retain the worst individual
observation across the three processes. All values are nanoseconds.

| Verified recipient condition | p50 | Process p50 range | p99.9 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Same owner | 191 | 190–191 | 260 | 5,108 |
| Different owner, active | 311 | 300–330 | 490 | 10,726 |
| Different owner, sleep observed | 3,666 | 3,666–4,147 | 11,808 | 3,338,834 |

The source holds the real virtual mutex until its successor is queued and the
required condition is observed. Actual task owner IDs and TIDs establish locality;
CPU masks 6–7 and individual owner pinning are verified. Command parks prevent
uncontended reacquisition. Each successful run checks generation, protected value,
queued handoff count, every chronological sample and clean shutdown. Each of these
nine recipients records 20,000 parks, including the command parks.

The sleep control observes Linux S/futex state **before release**, not atomically
at publication. Spurious wakeups and OS rescheduling remain possible. Its p50 is
about 12x the remote-active control here; this is a forced-state comparison, not
the frequency or cost distribution of states in the production mutex workload.
The fixture has one FIFO successor, not a general multi-waiter fairness result.

## Counters materially affect sleeping controls

The original default panel has three processes at both 1,000 and 10,000 handoffs.
Its long p50 medians with `perf stat` are 191 / 320 / 10,566 ns; p99.9 medians are
281 / 501 / 46,280 ns. Its worst observations are 4,707 / 7,221,885 / 50,990,719 ns.
These are retained, **not substituted for the counter-free table**.

The [current-source collector crossover](quiet-checkpoint-review.md) separately
shows large counter-on/off effects in lifecycle and TCP. That motivated the nine
counter-free fixture controls. This fixture's two collection blocks are not an
interleaved crossover and do not establish an underlying PMU/kernel mechanism.
In particular, 10.6 us must not be presented as the default sleeping-handoff cost.

Whole-process cycle medians at 10,000 handoffs are approximately 19,702 local,
44,338 remote-active and 376,666 sleep-observed per handoff. They include setup,
source/keeper yields, command parks, procfs checks, shutdown and raw sample output.
They are **not isolated lock cycles**; increasing work count does not remove the
control work that scales with handoffs.

## Separate diagnostic evidence

Three diagnostic processes per mode, 1,000 handoffs each, use the existing
profiling feature. No timings from this executable enter the headline table.

| Mode | Routing counts | Native-wait invocations |
| --- | --- | --- |
| Local | 2,000 local, zero shared | Two |
| Remote-active | Source: 1,999 shared; recipient: one local | Two per carrier |
| Sleep-observed | Source: 1,999 shared; recipient: zero routes | Recipient: 1,001–1,003; source: two |

Counts include command traffic. A native-wait invocation is a condition-variable
call, not proof that the OS actually descheduled the carrier. A shared route alone
does not identify a remote owner; the verified task identities supply that fact.

## Decision and evidence

Keep direct ownership and cancellation recovery. No evidence here justifies
another ownership-slot representation or increased polling. Production state
frequencies, useful acquisitions versus retries, multi-waiter age/acquisition
spread, loaded tails and the cost of intervening carrier work remain separate
attribution requirements. Larger maximum outliers are retained, not explained
away by a quiet guest guard. Physical-host isolation/frequency remain unknown.

The fixture is clean detached `80b3ba1`; the current perf checkout was `bfdef5e`.
Intervening production-module edits only declare sibling tests; mutex, execution,
scheduling, placement and polling behavior are unchanged. Native fixture source:
`f0ce5246d87b7846b36b0d1ddc8f3f7c1b95e5509922f20f2fe8b4bbdfe9b9c5`.
Default executable:
`8964d0d5983971d834a2c6543859a8e82224f032d62ec23870432b108a651d47`.
Diagnostic executable:
`c75b00416f82f854094d6048acddbd6a9306143e687632cbef2d28740903ecce`.

The preceding qualification remains fourteen canonical gates, including default
native debug/release, with four evidence-parser tests. This measurement-only
slice does not claim a new canonical run, ARM64 execution or sanitizer support.

The [durable 393-file bundle](evidence/mutex-mechanism-quiet-f0ce5246.tar.gz)
preserves all raw chronological samples, process counters, diagnostic reports,
commands, guards, source/binary identities, analyzers, fixture patch and preceding
receipt. Every internal hash verifies. Archive SHA-256:
`f66969872a14303c1ea2036e3e5bcc976f8bab24e67a7b7a5d95a550fd970c8b`.
