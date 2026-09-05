# Shared bounded-channel control

This is a benchmark-only slice against `741028506fe151abf6bc6520d3c65411f50d3e1c`.
Runtime source, atomics, channel selection and `zrail.lock` are unchanged.
Historical paired-channel contracts remain available. Their vthread helper moves
to a separate file to keep the engine module under 300 lines.

The new `channel-mpmc` control measures a single bounded channel with equally many
producers and consumers. The minimum population is four tasks. Each producer sends
distinct `usize` identifiers; each consumer records an equal quota in its own vector.
Normal admission alternates consumers and producers, without a placement hint,
barrier, per-round diagnostic snapshot or extra shared measurement lock.

One reported operation is one delivered value, including both send and receive:
`tasks / 2 * messages-per-producer` transfers per round. This is not the denominator
of the historical two-direction ping-pong. The capacity and exact per-direction
waiter limit are printed. May explicitly rejects this vthread-only control; no
different channel contract is silently substituted.

Every warm-up and measured round must prove exact per-consumer count, valid identifier
range and global uniqueness. Together these establish that every expected value was
delivered once. Validation and vector destruction occur after elapsed timing and
allocation recording stop. Receiver vector allocation and recording remain in the
end-to-end interval. Whole-process hardware counters include all validation, warm-up
and shutdown; they are not isolated channel-instruction counts.

The control does not measure individual operation latency, open-loop backpressure,
fairness spread or worst waiter age. Fixed consumer quotas establish complete work,
not a loaded fairness guarantee. No speedup is claimed from the qualification smokes.

## Qualification and preservation

Source SHA-256:
`a373785e0c69705762c94f7deeef176482457ca4f306baca9c75494153d47874`.

- All 11 canonical project gates passed under receipt
  `/root/.cache/zcheck/run-1788643028-137829244-2443491/receipt.json`.
- All 44 standalone benchmark tests and all-target/all-feature warning-denying
  Clippy passed; benchmark formatting is clean.
- Tests reject missing evidence, duplicate/out-of-range values, wrong consumer
  counts and quotas, invalid CLI dimensions, and May through both entry routes.
  A report-level negative test proves measured rounds are checked after warm-up.
- Optimized native CLI smokes pass for one/four carriers and capacities 1/64/1024,
  with exact delivery in every round; May exits with the explicit rejection.

The [source-keyed bundle](evidence/shared-channel-control-a373785e.tar.gz) preserves
commands, source and binary hashes, tests, smokes and the canonical receipt. Its
digest is in [the evidence index](evidence/README.md). The immutable baseline binary
is retained locally before the channel eligibility experiment begins.
