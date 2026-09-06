# Cross-runtime join: preserve typed failure and ownership evidence

The interrupted foreign-join finding is closed as a test-gate/oracle failure, not
a demonstrated runtime ownership defect. Join, completion, cancellation, deadline
and scheduling behavior are unchanged.

The historical loaded trace explicitly shows the target's five-second native
test gate timing out and panicking, the waiter's deadline-only assertion failing,
then a disconnected handle-return channel. The actual returned join error was not
printed. That log establishes the fixture failure; it does not prove that a
runtime interruption lost the handle. Its unique OS scheduling history cannot be
recovered and is not silently attributed to host load.

## Ordered replacement

Two distinct runtime owners execute real native fibers on one ordinary test
thread. Their task IDs deliberately collide, exercising runtime identity as well
as ownership. The foreign target first parks; the waiter then parks on the exact
Join(target) generation. Production timer expiry publishes that generation before
any resumption and rejects a late ready selection.

The waiter returns both its typed `DeadlineExceeded` and the target handle. There
is no native gate or handle-return channel. The target remains unfinished and
`WouldBlock`, then explicit release permits completion: `42` is recovered exactly
once and another take reports `ResultAlreadyTaken`. Both owners drain and retire
their timers; root deadline policy is checked separately.

A second scenario completes the target with an intentional panic before running
an overdue joiner. The completed result remains `TaskPanicked`; the next checkpoint
reports `DeadlineExceeded`. This reproduces the old oracle's invalid assumption
with ordered typed evidence, without inventing the missing error text in its log.

## Negative controls and qualification

- Omitting timer selection fails `Stale` versus `Published` before resumption.
- Deliberately consuming join ownership on interruption makes the unfinished
  target report `ResultAlreadyTaken` and fails the recovery test.
- Applying the old deadline-only oracle to the completed panic fails while
  preserving the actual `TaskPanicked` outcome.

All mutations are restored and the final test is byte-compared with its saved
source. The shared ordinary-thread owner is extracted from the preceding timer
test, whose full ordered coverage passes independently and canonically. The
architecture delta is two test files/ten contexts, zero grants or debt.

Canonical receipt `run-1788697218-738782565-3073203` passes all fourteen tasks with
repository state preserved: 543 default-native runtime tests in debug and release,
572 with all features, two explicitly manual/performance tests ignored.
Source SHA-256:
`d4bf245064800d062c0c31e5cbdeec8feecab44c56aff3a0aa9be902dbf19769`.
Twenty further debug and twenty release invocations on CPU 6 pass both tests:
**eighty additional native test executions**. The intentional target-panic trace
is preserved, including in successful runs.

The [170-file verified bundle](evidence/cross-join-order-d4bf2450.tar.gz) contains
the original loaded failure, all negative controls, restored source, the extracted
driver/timer regression, canonical receipt, exact commands, source/binary hashes
and every repetition. Archive SHA-256:
`65f11cf64108d634b03d15a234dd039a3aa62872d2aba466ce36623184fc83dd`.

This is cross-runtime identity and ownership proof, not concurrent OS-carrier or
sleep/wake performance qualification. Native ARM64, sanitizers, the large mixed
lifetime matrix and the refreshed May comparison remain separate. Two loaded
findings remain unclassified: inbox refill and deadline-first joins; the historical
cancellation-history timing excursion remains separate release-performance work.
