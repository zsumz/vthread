# Release 0.0.2

Status: preparing `0.0.2` for early evaluation; crates.io publication is pending.
This version does not carry a production-readiness verdict. The
[recorded qualification](#recorded-qualification) and
[remaining release gates](#remaining-release-gates-and-limitations) define its
current evidence and limits.

The runtime retains carrier affinity, both cancellation checkpoints, exact wait
generations, bounded resource accounting, structured scope ownership, and
cancellation-safe direct mutex ownership transfer.

## Scope

Version `0.0.2` retains the `0.0.2-rc.2` runtime: native guarded stacks and execution
reuse, compact carrier-owned task storage, resident synchronization waits, and
owner-routed wakes.
It also includes bounded wake cohorts and admission service, deferred publication
and cleanup, cached ingress visibility, routed timers, and revocation-epoch
maintenance. Diagnostic features remain opt-in; default builds do not enable
timing instrumentation.

Held capacity, readiness, channel, mailbox, polling, and lazy-fault experiments
are not included in this version. Their earlier performance results
are not claims about this version. The standalone benchmark workspace measures
only this runtime. HTTP remains outside the core project.

Historical experiment reports and raw evidence remain on the preserved
`perf/scheduler-hot-path` branch at `6e7121cd00dc7b3efdc93e128b55f83cdf89c2d0`.
They are not part of the release checkout or distributed crates. Git history is not
rewritten.

## Correctness repairs

The source audit found and repaired four concrete correctness defects, with
negative regressions recorded before the repairs:

| Defect | Repair |
| --- | --- |
| Cleanup used dispatch policy as a queue iterator, allowing repeated visits, missed runnable work, missed reclamation, and incorrect retained-task recovery. | Cold maintenance inspects both queue lanes exactly once, preserving normal dispatch policy and live-task visibility in intermediate snapshots. |
| A typed native-stack context callback could suspend while holding a reference whose owner was valid for only one resume. | Lending that context prevents its fiber from suspending and restores the mount through nested calls and panic. |
| Dropping an unstarted fiber installed a suspension target whose parent context had never been saved. | Reclamation preserves the actual executing mount. |
| Forced-unwind cookie block exhaustion advanced the allocator and could reuse identities. | Exhaustion remains permanent, including after caught panics. |

These are correctness repairs, not optional performance promotions. Independent
cross-reviews found no additional concrete concern in the repairs. Negative
controls and final-tree qualification are recorded separately; neither the audit
nor the passing tests establish an exhaustive runtime proof.

## Qualification contract

| Verification | Required coverage |
| --- | --- |
| Canonical `zcheck run check` | Native debug and release workspace tests; all-feature tests; documentation and compile-fail examples; source, layout and architecture policy; application evidence validation; public-API load and failure smoke tests. |
| Standalone benchmark, also required by `zcheck run check` | Formatting, Clippy, default tests and all-feature tests. A workspace-only pass does not qualify this separate manifest. |
| Additional release qualification | Distributable packages and their dependency closure, longer mixed traffic, full application qualification, and exact-source native execution on both advertised targets. |

Package creation is not publication. The publication order is `vthread-stack`,
`vthread-sync-core`, `vthread`, then `vthreads`; the lab, benchmark and reference
packages remain unpublished.

## Recorded qualification

The `0.0.2` code and manifests passed all 18 canonical gates and all 13 standalone
reference tests locally on Linux x86-64. This is not native macOS execution or
production-readiness evidence.

### Historical RC evidence

These historical results describe `0.0.2-rc.2` on Linux x86-64 with Rust 1.96.1
and the default native engine unless a feature set is named. They do not attest to
newly versioned `0.0.2` package bytes. The archived code/manifest digest is:

`8ac4c314fdfe944a53ddf4395b1feb4cfa5bd18465f0c559201823030e3ef73d`.

| Check | Result |
| --- | --- |
| Canonical `zcheck run check` | All 18 gates passed; repository state preserved |
| Native stack | 79 tests passed in both debug and release |
| Standalone benchmark | 45 default and 53 all-feature tests passed; formatting and Clippy passed |
| Standalone reference | 13 tests passed |
| Full application matrix | 22 cases passed: eight loads, eight fixed-arrival cases, six failure rounds |
| Bounded mixed soak | Three 30-second processes passed; 530,361 task lifetimes completed and reclaimed |
| Distributable crates | All four clean archives built, verified and independently audited |

### Workload and source boundaries

The soak covered one/four carriers and 64/1,024-task batches on the preceding
digest `a794281116bfb7708c564345e5b740d9da4bb70389e1502c711a570088f5f3ed`.
The only subsequent Rust change corrected cleanup documentation, not executable
code. Final canonical, application and reference checks were rerun on the archived
digest above.

The application panel uses concurrency 1/16/64/256, 128 closed-loop rounds, three
failure rounds per carrier count, and 256 offered arrivals at 2,000/second. Its
timing samples are local observations, not controlled-host tail acceptance.

The two existing manual performance probes are intentionally excluded from the
canonical test count; mandatory cancellation semantics and bounds still pass.

### Package and evidence identity

The initial package audit found a missing license file in `vthread-sync-core`;
the root Apache-2.0 license was added byte-for-byte. Final archives match
the clean source commit `58976649f5fb2bf716cb5e2a3694dbb6bf2b2548`, include every
package's license, and have exact internal version pins and archive checksums.

Raw qualification artifacts are archived separately from the source checkout.
The [historical evidence index](https://github.com/zsumz/vthread/blob/12bac5291b4c262a01ace65330903789880e15cf/release-evidence/0.0.2-rc.2/README.md)
records the preserved bundle's hashes and replay instructions.

The `0.0.2` version metadata, documentation and unsupported-platform diagnostic
wording are outside that archived snapshot. The recorded hashes identify the
historical artifacts, not newly packaged files.

## Remaining release gates and limitations

| Area | Open requirement or limitation |
| --- | --- |
| Historical refill stall | The coalesced-inbox refill stall remains unclassified. Its test captures accepted, queued, started and completed work before cleanup, but passing reruns cannot reconstruct the missing historical state. The cleanup repairs are not assumed to explain it. |
| Cancellation history | Semantic bounds and cancellation paths remain mandatory tests. The historical wall-time excursion remains separate performance evidence; `zcheck run perf-cancellation-history` retains its explicit optimized guard. |
| macOS ARM64 | Current-source native execution is required. A workflow definition or cross-compilation is not an execution receipt. |
| Alternate-stack sanitizers | Hooks are not qualified. Ordinary compiler sanitizer flags do not establish support for the native context-switch boundary. |
| Scale and sustained load | Large simultaneous populations, the full mixed-lifetime stress target, memory footprint, loaded tails and controlled-host idle CPU require separate qualification. Short smoke runs do not replace it. |
| Scaling costs | Wake-depth observation has provisioned-capacity-dependent cost; the readiness driver still reconciles registration maps. Neither held scaling candidate is included. |
| Performance acceptance | No dedicated performance host is currently available. Local timing is observational, with no new performance acceptance or latency guarantee. |

These items must remain visible when deciding whether to merge, tag or publish.
Preparing `0.0.2` sources does not waive unresolved correctness or platform gates.
