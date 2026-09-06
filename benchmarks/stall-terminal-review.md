# Terminal-credit stall accounting: ordered repair

The stall error now counts the same live tasks as its retained diagnostic report,
not admission credits that can briefly outlive terminal completion. This is a
correctness repair on the optional stall-failure path, **not a performance win**.
No scheduling, polling, completion/join ordering or admission retirement changes.

## Failed checkpoint and negative control

After restoring the retained `e344a60e` runtime, canonical run
`run-1788716551-875203881-3238339` failed native release test
`a_terminal_sibling_does_not_hide_an_indefinitely_parked_child`. Its assertion
expected `RuntimeStalled { active: 1 }` but did not print the actual error. That
receipt and every task log remain archived; a quiet rerun is not the explanation.

The new native test holds the existing `Completion::after_notify` test hook:

1. One child is observed parked; another completes and commits its terminal result.
2. The terminal carrier remains held before admission-credit retirement. The root
   observes two outstanding credits and successfully joins the terminal result.
3. A separate observer releases the carrier only after the stall report exists.

Against unchanged runtime code, the ordered test reports
`RuntimeStalled { active: 2 }` while its captured stall report contains exactly
one live task, `parked`. The original assertion therefore has a demonstrated
source-level counterexample, not merely a timing hypothesis. The original run's
unprinted primary error remains unrecoverable; this does not retroactively prove
its exact value.

The first fixture attempt failed configuration validation because its four-task
limit required lowering stack-cache capacity. It is retained separately and is
not counted as the negative control. The corrected fixture uses notification
ordering, not an elapsed-delay assumption or a longer stall timeout.

## Minimal correction and qualification

`Shared::wait_until` already captures nonterminal task snapshots when reporting a
stall. It now uses that captured task count when constructing `RuntimeStalled`.
There is no additional default-path scan, atomic operation, allocation or clock.
The original intermittent assertion also prints its actual error and snapshot on
any future failure. The ordered regression checks credit retention, the committed
join result, the live error count, complete reclamation and runtime reuse.

Final source:
`d38dcb8627f61e49f3f95e46c270b9349c6629365060ac98e2dab6bbb00bfa94`.
Canonical `run-1788717897-409009227-3259492` passes **all 14 gates**, preserving the
qualified worktree. Default-native debug/release each run 546 vthread unit tests
with two existing ignored probes, plus workspace integration/stack/model tests.
The all-feature diagnostic run passes 575 vthread unit tests. Qualification output
explicitly identifies `vthread-stack`; formatting, clippy, panic contract,
documentation, zrail and application smoke/evidence checks also pass. No grant or
lockfile change is needed.

[Durable bundle](evidence/stall-terminal-d38dcb86.tar.gz): 49 verified internal
hashes, original failed receipt, setup failure, intended negative, positive tests,
corrected canonical receipt/logs, both source archives and qualification binary
hashes. Every recorded test source is reconstructed from the retained archive and
its exact patch, with its digest verified. SHA-256:
`9bd89f6a08a15c9527d0e69f9886ee63ca9f9979e6126c67d042d6276d2b31f7`.

Current-source ARM64 execution, supported sanitizer integration, large simultaneous
and mixed populations, footprint, loaded tails and the other historical loaded
findings remain separate release obligations. The earlier May timing table is not
refreshed by this cold-path correction.
