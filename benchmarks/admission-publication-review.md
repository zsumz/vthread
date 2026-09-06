# Scan-free lone-admission publication: rejected

This follow-up tests the batch/publication interaction identified by the scan-only
screen. It does not change polling, fairness or ownership. The protected default
counter-free lifecycle screen still loses, so **no runtime change is retained**.

Base `4d7fe6a` has the retained source identity `e344a60e`. Candidate source:
`f406d0ca6403472ca49b53134ff78a215668b5aa56a12a005298e40b12d9eb91`.
It combines scan-only B with one guard: omit receive-time publication for a lone
owned admission when stall detection is disabled and no borrowed work exists.
The mandatory first-mount publication remains. Batched, borrowed, stall-enabled
and completion publication remain unchanged, as do all 640 polling probes.

## Protected screen and stop

The unchanged [predeclared rules](counter-free-acceptance.md) govern four balanced
A/B process pairs per collection/placement cell: four carriers, 1,000 tasks,
501 rounds, CPUs 0-3. All 32 planned processes complete; none is discarded.

| Placement / collection | Median paired time change | Median paired whole-process CPU change |
| --- | ---: | ---: |
| Default / bare | +14.01% | +20.14% |
| Default / counters | +15.51% | +24.45% |
| Pinned / counters | +14.67% | +20.20% |

Default bare crosses the early stop: three of four pairs exceed +10%. Pinned bare
is multimodal: baseline processes are approximately 926, 927, 343 and 340 ns/task,
while the candidate stays around 970–982. Its +96% median paired effect must not
be presented as one stable execution mode. Every observation remains in the raw
results. No confirmation, scaling panel or full candidate canonical follows the
stop. Eliminating this publication alone does not recover tight-capacity speed;
there is no justification here for another polling variant.

## Correctness and restored-baseline finding

The ordered negative control fails on scan-only B at duplicate publication,
with six positive controls passing. The guarded candidate passes 75 kernel tests
(one existing manual probe ignored), plus formatting, clippy, 47 benchmark tests
and seven analysis tests. Accounting includes local, remote and deferred wakes.
This is targeted candidate evidence, not full candidate qualification.

After restoring the retained runtime, canonical run
`run-1788716551-875203881-3238339` failed native release test
`a_terminal_sibling_does_not_hide_an_indefinitely_parked_child`. Its assertion did
not print the actual primary error. Native debug and all-feature tests passed;
the later dependent gates were blocked. The failed receipt and task logs are
preserved, not replaced by a quiet rerun. This is a separate current-baseline
correctness investigation, not an optional candidate regression. The restored
checkpoint is not claimed green from this run.

Candidate stash: `771294513b48af80db6d80f879451730a35f297a`.
[Durable evidence](evidence/admission-publication-f406d0ca.tar.gz) contains 382
verified hashes, both reconstructed source archives, the complete process plan
and outcomes, negative/positive tests and failed retained canonical receipt.
SHA-256: `cf6150ee2e2b7316242aa1e16813d5b60e1b0e4f267340283ca5b35581ebeb8b`.

The next independent performance slice remains bounded pre-dispatch wake work.
Capacity-independent maintenance and protected incremental readiness are still
open; neither has been recovered by these acceptance experiments.
