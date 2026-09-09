# Release candidate 0.1.0-rc.1

Status: production release candidate; this source snapshot does not itself authorize
tagging or publication.
`0.1.0-rc.1` is a production candidate for the intended `0.1.0` release on the
supported platforms within the documented boundaries. Its pre-1.0 version permits
planned compatibility breaks at minor versions; it does not lower the runtime's
correctness or qualification standard. The [recorded qualification](#recorded-qualification) and
[remaining release gates](#remaining-release-gates-and-limitations) define its
current evidence and limits.

The runtime retains carrier affinity, both cancellation checkpoints, exact wait
generations, bounded resource accounting, structured scope ownership, and
cancellation-safe direct mutex ownership transfer.

## Compatibility

The eventual `0.1.0` release and subsequent `0.1.x` releases will preserve public
API compatibility within `0.1`. Breaking public API or contract changes move to
`0.2`; `1.0` requires a separate durable-API commitment. Use
`vthread = "=0.1.0-rc.1"` to select this exact prerelease. After the final release,
use `vthread = "0.1"` to receive compatible `0.1.x` releases.

The exact dependency set includes `zio = "=0.0.1-dev.1"`. A normal vthread version
does not imply that every dependency has a stable-version contract.

## Inbox progress

Version `0.1.0-rc.1` closes two demonstrated notification-boundary gaps. An active
carrier treats published inbox depth as an authoritative receive obligation. A
parked carrier can be released by a later publisher while the first notifier is
delayed, and waiter registration rechecks the queue under its mutex before sleep.
Focused regressions fail when either half of that parked-carrier handoff is removed.

## Panic isolation

Version `0.1.0-rc.1` prevents one task from switching away while its carrier is
running a panic hook or unwinding a panic. When a suspension-capable operation
reaches that boundary, it returns `Error::SuspensionDuringPanic` before publishing
a wait generation, entering a resource queue, subscribing to task completion or
cancellation, or registering readiness or a native job. A rejected operation
cannot install the subsequent scheduler timer or wake registration. An explicit
`Parker` operation that reaches this preflight rejects before consuming a stored
permit. Private forced-unwind reclamation continues on the same task, runs its
destructors, and does not schedule a sibling.

## Scope

Version `0.1.0-rc.1` retains the established `0.0.2` runtime architecture: native
guarded stacks and execution reuse, compact carrier-owned task storage, resident
synchronization waits, and owner-routed wakes.
It also includes bounded wake cohorts and admission service, deferred publication
and cleanup, cached ingress visibility, an authoritative published-depth receive
fallback, routed timers, and revocation-epoch maintenance. Diagnostic features
remain opt-in; default builds do not enable timing instrumentation.

Held capacity, readiness, channel, mailbox, polling, and lazy-fault experiments
are not included in this version. Their earlier performance results
are not claims about this version. The standalone benchmark workspace measures
only this runtime. HTTP remains outside the core project.

Historical experiment reports and raw evidence remain on the preserved
`perf/scheduler-hot-path` branch at `6e7121cd00dc7b3efdc93e128b55f83cdf89c2d0`.
They are not part of the release checkout or distributed crates. Git history is not
rewritten.

## Correctness repairs

The release also carries the following behavior-isolated correctness repairs:

| Defect | Repair |
| --- | --- |
| Cleanup used dispatch policy as a queue iterator, allowing repeated visits, missed runnable work, missed reclamation, and incorrect retained-task recovery. | Cold maintenance inspects both queue lanes exactly once, preserving normal dispatch policy and live-task visibility in intermediate snapshots. |
| A typed native-stack context callback could suspend while holding a reference whose owner was valid for only one resume. | Lending that context prevents its fiber from suspending and restores the mount through nested calls and panic. |
| Dropping an unstarted fiber installed a suspension target whose parent context had never been saved. | Reclamation preserves the actual executing mount. |
| Forced-unwind cookie block exhaustion advanced the allocator and could reuse identities. | Exhaustion remains permanent, including after caught panics. |
| A task could suspend from a destructor or panic hook while the carrier's native panic state was active, exposing that state to a sibling or causing a nested panic-hook abort. | The shared stack boundary rejects suspension during panic handling; wait-capable paths reject before publishing state, and forced reclamation retains its private transfer path. |

These are correctness repairs, not optional performance promotions. Focused
regressions and mutation controls cover them; neither the audit nor passing tests
establish an exhaustive runtime proof.

## Qualification contract

| Verification | Required coverage |
| --- | --- |
| Canonical `zcheck run check` | Native debug and release workspace tests; all-feature tests; documentation and compile-fail examples; source, layout and architecture policy; panic-isolation regressions, including the subprocess-isolated panic-hook case; application evidence validation; public-API load and failure smoke tests. |
| Native-stack CI, on both targets | Default and optimized stack tests, including panic-time suspension rejection and forced-unwind reclamation, with source and binary identities preserved. |
| Standalone benchmark, also required by `zcheck run check` | Formatting, Clippy, default tests and all-feature tests. A workspace-only pass does not qualify this separate manifest. |
| Release CI, on both targets | Eight closed-loop loads, eight fixed-arrival cases at 2,000 arrivals/second, six failure rounds, then offline verification of all four distributable packages. Logs and package archives are uploaded. |
| Sustained mixed-lifetime closeout | One externally supervised 3,600-second Linux process at four carriers and 4,096 tasks; at least 10 million task lifetimes; exact accounting for completion, parks, wakes and stack acquisition; clean service and shutdown drain. |
| Distribution closeout | Audit clean candidate archives, licenses, normalized manifests, exact internal dependency closure and source identity. After publication, run the README example in a fresh registry-only consumer before announcement. |

Package creation is not publication. The publication order is `vthread-stack`,
`vthread-sync-core`, `vthread`, then `vthreads`; the lab, benchmark and reference
packages remain unpublished.

## Recorded qualification

Qualification belongs to the immutable commit being promoted. Its workflow runs,
artifact identities, archive hashes, and sustained-run receipt are kept in the
source-keyed candidate evidence bundle and must be attached to the release entry;
they are not written back into this file because doing so would create a different,
unqualified source commit. Historical results below establish the baseline only.

### Historical 0.0.2 baseline

Commit `6882d708207e5549b86f4b76832d7f878f45799c` (`0.0.2`) has successful jobs on
both advertised platforms:

| Qualification | Linux x86-64 | macOS ARM64 |
| --- | --- | --- |
| Canonical repository check | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732304/job/101829285225) | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732304/job/101829285542) |
| Native-stack debug | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732339/job/101829285608) | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732339/job/101829285669) |
| Native-stack release | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732339/job/101829285510) | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732339/job/101829285607) |
| Application load/failure | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732304/job/101830730576) | [Passed](https://github.com/zsumz/vthread/actions/runs/34149732304/job/101830730549) |

Public job metadata confirms successful execution, including native host checks.
The baseline application configuration ran eight closed-loop cases and six failure
rounds, **not** the full 22-case matrix: it supplied no fixed-arrival arguments.
That scope follows from the pinned command, successful step and runner validation;
raw CI logs and artifact downloads were not accessible during this closeout.

These results qualify only the historical baseline. The immutable publication
candidate must satisfy the qualification contract above.

### Earlier 0.1.0 candidate

An earlier `0.1.0` candidate passed all 18 canonical gates locally on Linux x86-64
with Rust 1.96.1, including native debug/release tests and doctests. That candidate's
code/configuration digest is:

`801397809bd0bcb1aa990d583764029c4db7c668f446ca6fe01b68f17847b730`.

Clean commit `46c9ce6e0d358f9ff19f620e2f6ec2f075507476` additionally passed:

| Check | Result |
| --- | --- |
| Full application matrix | 22 cases: eight closed-loop loads, eight fixed-arrival cases, six failure rounds |
| Standalone reference | All 13 tests passed, including the updated `52` examples |
| Distributable packages | All four archives built offline from packaged contents and passed an independent audit |

The audit verifies committed source bytes, licenses, normalized manifests, exact
internal pins, registry dependency closure and sibling archive checksums. Evidence
and archives are kept outside the source checkout. These results are not a
registry-consumer check, a controlled performance result or a stability verdict.

The required release jobs run the full 22-case application matrix and package
verification; their uploaded artifacts identify each run and target. Rebuilding
after any source commit changes requires a fresh archive audit, even for
documentation-only edits.

### Historical RC evidence

These historical results describe `0.0.2-rc.2` on Linux x86-64 with Rust 1.96.1
and the default native engine unless a feature set is named. They do not attest to
newly versioned `0.1.0-rc.1` package bytes. The archived code/manifest digest is:

`8ac4c314fdfe944a53ddf4395b1feb4cfa5bd18465f0c559201823030e3ef73d`.

| Check | Result |
| --- | --- |
| Canonical `zcheck run check` | All 18 gates passed; repository state preserved |
| Native stack | 79 tests passed in both debug and release |
| Standalone benchmark | 45 default and 53 all-feature tests passed; formatting and Clippy passed |
| Standalone reference | 13 tests passed |
| Full application matrix | 22 cases passed: eight loads, eight fixed-arrival cases, six failure rounds |
| Bounded mixed soak | Three 30-second processes passed; 530,361 task lifetimes completed, with all service and shutdown drain assertions passing |
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

Subsequent version metadata, documentation and unsupported-platform diagnostic
wording are outside that archived snapshot. The recorded hashes identify the
historical artifacts, not newly packaged files.

## Remaining release gates and limitations

| Area | Open requirement or limitation |
| --- | --- |
| Cancellation history | Semantic bounds and cancellation paths remain mandatory tests. The historical wall-time excursion remains separate performance evidence; `zcheck run perf-cancellation-history` retains its explicit optimized guard. |
| Distribution qualification | Preserve exact-source both-target CI, sustained-run and final archive audit results. Packaged README links are pinned to the intended immutable `v0.1.0-rc.1` tag; create that tag only after the linked package bytes are requalified. After authorized publication, run a fresh registry-only README consumer before announcement. |
| Alternate-stack sanitizers | Hooks are not qualified. Ordinary compiler sanitizer flags do not establish support for the native context-switch boundary. |
| Scale and sustained load | Candidate closeout requires one continuous Linux process for 3,600 seconds at four carriers and 4,096 tasks, with at least 10 million completed task lifetimes and exact drain accounting. Larger simultaneous populations, cross-platform sustained runs, memory footprint, loaded tails and controlled-host idle CPU remain unqualified. |
| Scaling costs | Wake-depth observation has provisioned-capacity-dependent cost; the readiness driver still reconciles registration maps. Neither held scaling candidate is included. |
| Performance acceptance | No dedicated performance host is currently available. Local timing is observational, with no new performance acceptance or latency guarantee. |

Promotion of this candidate to `0.1.0` does not require every possible performance
or scale objective to be complete when those claims are excluded. It does require
accurate boundaries and exact-candidate qualification. Publication is a separate
maintainer-authorized action.
