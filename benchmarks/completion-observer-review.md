# Target observer and completion accounting

This test-only slice starts at `2abae58d63fdce689251f26603dca1df98402fd3`.
Wake-publication probes were preserved separately while diagnosing a canonical
failure. No production completion, scheduler, wait, checkpoint or batching policy
change is retained here.

## Captured failure and ordered explanation

Canonical run `run-1788633793-130943458-2296893` failed the existing
`target_waiter_forces_prompt_completion_publication` assertion: scope completed
count was **0**, expected **1**. The other 525 all-feature runtime tests passed,
including the new publication probes. A quiet rerun is not the explanation.

The production completion path makes the reclaimed task's completion flag visible
before queuing its scope-accounting update. An OS target waiter may then observe
completion and drop its registration. If no target waiter remains when the carrier
queues the update, `may_defer_completion()` permits the normal bounded batch.

A deterministic test reproduces that order using the real `Shared::wait`, target
registration, completion flag and `Kernel::queue_completion`. Holding the control
state lock orders the observer's first target inspection after completion becomes
visible. The observer exits before the update is queued. The captured tuple is:

```text
(target_done, may_defer, batched, scope_completed) = (true, true, 1, 0)
```

Asserting the old test's required last component of 1 fails. Explicitly flushing
the batch then publishes that completion, and completing the sibling drains the
scope. No deadline, sleep, extended timeout or scheduler-load assumption is used
to establish the disputed order. This component test does not execute a fiber;
the corresponding kernel policy test does.

Task-result visibility and physical stack reclamation are not weakened. The public
join contract waits for the reclaimed task's result, not an independently batched
scope counter. The original test assumed its waiter would remain registered until
accounting was queued; its registration observation did not establish that lifetime.

## Repaired qualification

The real-kernel test now keeps a target waiter registered for the **second** task
while dispatching the first. The waiter therefore cannot legitimately leave before
the first completion is queued. The test still requires immediate scope accounting
while a target waiter exists. It then runs the target and checks waiter completion.
Both tasks and the observer are drained before checking the disputed counter.

A deliberately removed live-waiter flush condition is used as a negative control:
the replacement must reject batching while that registration remains alive. The
condition is restored before retained-tree qualification. The separate ordered
observer-retirement test records why batching is allowed after the waiter exits.

The removed-flush negative control failed as required with completed count 0 rather
than 1. Final source SHA-256 is
`f8f1f6f5f6ceeae54e372cdb62d3da86bbbe1eab3f400646a37b5aea5316bffc`.
All 11 canonical gates passed under receipt
`/root/.cache/zcheck/run-1788634449-691951184-2302619/receipt.json`.
The separate default-native workspace suite passed 506 runtime tests (one ignored),
70 stack tests and all other workspace suites. Optimized native completion tests
also passed: 28 selected tests. The architecture lock is unchanged.
Both ordered boundary tests additionally passed 50 independent debug-process runs
each with a one-CPU mask (100 passing invocations total).

The five-second receive timeout is only a failure bound after the target has run;
it does not establish registration, completion ordering or expected accounting.
The existing task-level prompt notification test remains unchanged.

## Evidence and limits

Raw failure, negative-control patches, commands, source identity, binary hashes
and final qualification are archived in
[`completion-observer-f8f1f6f5.tar.gz`](evidence/completion-observer-f8f1f6f5.tar.gz),
SHA-256 `b080b4bc24ed6a274cc2755433cb928e052dbfc338c71be341e416ba7f3f4070`.
An initial fixture build
rejected a two-task configuration whose default stack cache was too large; that
setup failure is retained separately and is not the completion counterexample.

The publication-probe branch also had an earlier run with 11 passing gates but a
repository-state rejection: its inventory refresh overlapped qualification. That
receipt is retained as rejected, not treated as a valid canonical pass.

This repair explains the captured completion assertion. It does not resolve the
other loaded-suite findings listed in [wait-qualification-review.md](wait-qualification-review.md),
the paused-publisher liveness dependency, or the capacity/idle performance issue.
No new May comparison, throughput gain or stable-release claim is made.
