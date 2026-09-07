# 0.0.2-rc.2 preparation

This is a release-candidate preparation branch, not a published release or a stable
release verdict. The runtime retains carrier affinity, both cancellation
checkpoints, exact wait generations, bounded resource accounting, structured
scope ownership, and cancellation-safe direct mutex ownership transfer.

## Scope

Retained work includes native guarded stacks and execution reuse, compact
carrier-owned task storage, resident synchronization waits, owner-routed wakes,
bounded wake cohorts and admission service, deferred publication/cleanup, cached
ingress visibility, routed timers, and revocation-epoch maintenance. Diagnostic
features remain opt-in; default builds do not enable timing instrumentation.

Held capacity, readiness, channel, mailbox, polling, and lazy-fault experiments
are not promoted by this release preparation. Their earlier performance results
are not claims about this candidate. The standalone benchmark workspace measures
only this runtime. HTTP remains outside the core project.

Historical experiment reports and raw evidence remain on the preserved
`perf/scheduler-hot-path` branch at `6e7121cd00dc7b3efdc93e128b55f83cdf89c2d0`.
They are not part of the RC checkout or distributed crates. Git history is not
rewritten.

## Review findings

The source audit found and repaired four concrete correctness defects, with
negative regressions recorded before the repairs:

- Cleanup used dispatch policy as a queue iterator, allowing repeated visits,
  missed runnable work, missed reclamation, and incorrect retained-task recovery.
  Cold maintenance now inspects both queue lanes exactly once without changing
  normal dispatch policy or hiding live tasks from intermediate snapshots.
- A typed native-stack context callback could suspend while holding a reference
  whose owner was valid for only one resume. Lending that context now prevents
  suspension of its fiber and restores the mount through nested calls and panic.
- Dropping an unstarted fiber installed a suspension target whose parent context
  had never been saved. Reclamation now preserves the actual executing mount.
- Forced-unwind cookie block exhaustion advanced the allocator and could reuse
  identities. Exhaustion now remains permanent, including after caught panics.

These are correctness repairs, not optional performance promotions. Independent
cross-reviews found no additional concrete concern in the repairs. Negative
controls and final-tree qualification are recorded separately; neither the audit
nor the passing tests establish an exhaustive runtime proof.

## Qualification contract

`zcheck run check` requires native debug and release workspace tests, all-feature
tests, documentation and compile-fail examples, source/layout/architecture policy,
application evidence validation and public-API load/failure smoke tests. It also
requires standalone benchmark formatting, Clippy, default tests and all-feature
tests. A workspace-only pass does not qualify the separate benchmark manifest.

RC preparation additionally checks distributable packages and their dependency
closure, longer mixed traffic, full application qualification, and exact-source
native execution on both advertised targets. Package creation is not publication.
The publication order is `vthread-stack`, `vthread-sync-core`, `vthread`, then
`vthreads`; the lab, benchmark and reference packages remain unpublished.

## Local qualification

Final code/manifest digest:
`8ac4c314fdfe944a53ddf4395b1feb4cfa5bd18465f0c559201823030e3ef73d`.
Linux x86-64, Rust 1.96.1, default native engine unless a feature set is named.

| Check | Result |
| --- | --- |
| Canonical `zcheck run check` | All 18 gates passed; repository state preserved |
| Native stack | 79 tests passed in both debug and release |
| Standalone benchmark | 45 default and 53 all-feature tests passed; formatting and Clippy passed |
| Standalone reference | 13 tests passed |
| Full application matrix | 22 cases passed: eight loads, eight fixed-arrival cases, six failure rounds |
| Bounded mixed soak | Three 30-second processes passed; 530,361 task lifetimes completed and reclaimed |
| Distributable crates | All four clean archives built, verified and independently audited |

The soak covered one/four carriers and 64/1,024-task batches on the preceding
digest `a794281116bfb7708c564345e5b740d9da4bb70389e1502c711a570088f5f3ed`.
The only subsequent Rust change corrected cleanup documentation, not executable
code. Final canonical, application and reference checks were rerun on the digest
above. The application panel uses concurrency 1/16/64/256, 128 closed-loop rounds,
three failure rounds per carrier count, and 256 offered arrivals at 2,000/second.
Its timing samples are local observations, not controlled-host tail acceptance.

The initial package audit found a missing license file in `vthread-sync-core`;
the root Apache-2.0 license has been added byte-for-byte. Final archives match
clean source commit `58976649f5fb2bf716cb5e2a3694dbb6bf2b2548`, include every
package's license, and have exact internal version pins and archive checksums.
The two existing manual performance probes are intentionally excluded from the
canonical test count; mandatory cancellation semantics and bounds still pass.

[Durable qualification evidence](release-evidence/0.0.2-rc.2/README.md) contains
the source snapshot, raw logs, negative controls, receipts and final crate archives.

## Remaining release gates and limitations

- The historical coalesced-inbox refill stall remains unclassified. Its test now
  captures accepted, queued, started and completed work before cleanup; a passing
  rerun cannot reconstruct the missing historical state. The cleanup defects
  above are not assumed to explain that separate finding.
- Cancellation-history semantic bounds and cancellation paths are mandatory
  tests. The historical wall-time excursion remains separate performance evidence;
  `zcheck run perf-cancellation-history` retains its explicit optimized guard.
- Current-source macOS ARM64 execution is required. A workflow definition or
  cross-compilation is not an execution receipt.
- Alternate-stack sanitizer hooks are not qualified. Ordinary compiler sanitizer
  flags do not establish support for this native context-switch boundary.
- Large simultaneous populations, the full mixed-lifetime stress target,
  memory footprint, loaded tails and controlled-host idle CPU remain separate
  qualification work. Short smoke runs do not replace those requirements.
- Wake-depth observation still has provisioned-capacity-dependent cost and the
  readiness driver still reconciles registration maps. Neither held scaling
  candidate is silently included here.
- No dedicated performance host is currently available. Local timing remains
  observational, not a new performance acceptance or latency guarantee.

These items must remain visible when deciding whether to merge, tag or publish.
Preparing RC sources does not waive unresolved correctness or platform gates.
