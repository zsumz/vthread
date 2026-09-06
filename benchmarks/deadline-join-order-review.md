# Deadline-first joins: selection survives late child completion

The deadline-first join finding is closed as an invalid test ordering/observer
oracle, without changing production join, wait, timer or scheduler behavior.
The original log captured only a disconnected test-gate receive; its parent result,
particular handle-type subcase and unique scheduling history were not preserved.
This is not evidence of a repaired production ownership defect.

## Ordered winner and ownership proof

Separate native tests cover transferable and genuinely borrowed child handles.
Both tasks park with their inherited deadline. Production expiry publishes the
parent's exact park generation; a late ready selection loses, and neither task
has completed or resumed.

The ordinary driver retains exactly the parent's already-selected wake notice,
including its task, route and generation. It delivers the child's notice to the
unchanged kernel, which reclaims that child. The real completion hook reports
**zero newly selected joiners** while the parent is still parked with its original
published generation. Only then does the driver deliver the parent's notice.

Both reports preserve `DeadlineExceeded` on the interrupted join, `42` on the next
join, and `ResultAlreadyTaken` thereafter. Checkpoint/root policy still report the
deadline. The borrowed case also verifies its child write, local policy and complete
borrowed-stack retirement. All tasks, waits and timers drain.

This is a bounded test-only delivery delay, not a new runtime mailbox or ready
policy. Real clocks and exact generations are unchanged. It proves a controlled
native interleaving, not OS-carrier scheduling latency or fairness.

A third case accepts a parent before expiry but mounts it afterwards. Its local
scope correctly skips the callback and returns the inherited deadline. Parent
completion orders destruction of the unused observer sender: nonblocking receive
reports `Disconnected` alongside `Ok(Err(DeadlineExceeded))` and zero parks. This
disproves the old mandatory-observer assumption without inventing the missing
historical parent result.

## Negative controls and qualification

- Omit timer selection: both handle cases fail `Stale` versus `Published`.
- Disregard a selected park error when child completion is now visible: both
  cases incorrectly consume `42` on the interrupted join and fail with full typed
  reports retained.
- Require the unused observer message: the valid local deadline result and
  disconnected sender fail that old oracle; both ownership cases still pass.

All mutations are restored and the final test matches the saved passing source.
Existing completion-first and cancellation-first tests remain unchanged. The
architecture delta is one separate test file/five contexts, zero grants or debt.

Canonical receipt `run-1788698472-298340359-3103362` passes all fourteen tasks with
repository state preserved: 545 default-native runtime tests in debug/release and
574 with all features, with two explicitly manual/performance tests ignored.
Source SHA-256:
`104957c55ec44a95c31eea843ecbc303fec2e3df4b9489322725ba14acfd3528`.
Twenty further debug and twenty release invocations on CPU 6 pass all three tests:
**120 additional ordered native test executions**.

The [166-file verified bundle](evidence/deadline-join-order-104957c5.tar.gz)
preserves the original failure, all negative controls and restored source,
canonical receipt/output, exact commands, source/binary identities, every
repetition and the bounded-delivery replay ledger. Archive SHA-256:
`5f341dba055025d7e6f203c2f01de84fc7f393c873a050ade13fdb5d72062f9d`.

Inbox refill remains the last unclassified loaded correctness finding. The
historical cancellation-history timing excursion remains separate performance
evidence. This slice claims neither a new May result nor ARM64, sanitizer or
large mixed-population release qualification.
