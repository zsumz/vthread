# Inbox refill: evidence before cleanup

Base `2b21d0f`. This is a qualified test/evidence repair, not a runtime speedup or
an established repair of the historical stall. Production admission, polling,
notification and scheduling are unchanged.

## Finding and retained change

The historical loaded run records an admission timeout followed by a producer
panic after cleanup stops the runtime. It did not capture accepted, queued,
started or completed work before that stop. The secondary rejection cannot
identify the primary cause. **The historical stall remains unclassified and
release-blocking.** Passing current runs do not reconstruct its missing state.

The replacement preserves 4,096 tasks, default capacity 65,536, remote queue
capacity 256 and the five-second admission watchdog. It explicitly observes the
real carrier registering its initial native wait before starting the producer.
It then records, before requesting cleanup:

- Admission result and a separately bounded completion-credit drain result.
- Accepted submissions at both ends of the observation window, queued starts,
  started bodies, end-of-body markers, completed scope credits and active tasks.
- Admission availability, scope failures, carrier snapshots, signal epoch and
  registered native waiter count.

These concurrent observations describe a window, not an atomic global snapshot.
End-of-body markers are not completion credits; published carrier counters can
lag. Cleanup follows observation and preserves producer/carrier panic reports
and any late admission result separately. Successful runs also require all 4,096
credits, zero active/queued work, no cleanup failures and carrier reclamation.
The old final-body notification no longer stands in for complete reclamation.

Scoped guards stop the worker before implicit joins if the observer fails. The
new completion watchdog bounds test failure; no runtime timeout was increased.

## Negative controls

| Deliberate break | Required failure actually observed |
| --- | --- |
| Suppress empty-to-nonempty inbox notification | Before cleanup: accepted = queued = active = 256; started = body returns = completion credits = 0; admission still enabled; epoch 0; one sleeping waiter; primary admission timeout |
| Request cleanup before observing progress | Capture rejected with `progress evidence captured after cleanup` |

The first control then records a late `RuntimeStopped` from the producer only
after cleanup. It demonstrates that the new evidence separates the original
failure from its secondary rejection. It does **not** establish that the old
loaded failure lost a notification. Both mutations are restored and their exact
patches, failures and passing restored source are archived.

## Qualification

All fourteen canonical tasks pass in `run-1788700201-426769387-3127326`, including
545 default-native runtime tests in debug and release and 574 all-feature runtime
tests; two intentionally ignored tests remain separate. Both configurations use
`vthread-stack`. The zrail change is one test-file inventory addition, with no
grants, revocations or debt.

Twenty debug and twenty optimized single-CPU repetitions pass: 163,840 additional
task lifetimes. Four further default-native full-library runs with 32 test threads
on CPUs 0–3 pass all 2,180 runtime test executions. Every refill records all 4,096
completion credits before cleanup. These runs reproduce the earlier oversubscribed
configuration, not its unknown historical schedule. They are correctness stress,
not a throughput screen or the planned large simultaneous-population burn-in.

Full tested source SHA-256:
`9dda2c9fce293598654d536a0bd75563f7bae7d0d55a55bc771779b79df5e608`.
The [durable bundle](evidence/inbox-refill-evidence-9dda2c9f.tar.gz) includes source
and binary identities, the test patch, original failure, both mutation controls,
canonical receipt/logs, all repeated commands/results and the replay ledger.
All 176 internal hashes were verified after extraction into a fresh directory.
Archive SHA-256:
`468a48f2f027b08344a40ba413313a5aa40e6d973e16dacb5c14ed2aebd8c1f4`.

## What remains

The fixture now preserves actionable evidence if refill stalls again. The old
stall is not waived, classified as host noise or called a proven production bug.
It remains a release blocker alongside the separately unexplained historical
cancellation-history timing excursion and the outstanding architecture,
sanitizer, large-lifetime and performance qualification.
