# Releases

The `0.1.0-rc.1` runtime was signed off at
[`c4b2380`](https://github.com/zsumz/vthread/commit/c4b2380138b9f7f7384b2da9cb4ee9803e229588).
Release automation uses [zrelease](https://github.com/zsumz/zrelease), pinned to
`1993b45bfae19a00e61a9138565b79e029c2d7f4` in
[the Rehearse workflow](.github/workflows/rehearse.yml) and
[the Release workflow](.github/workflows/release.yml).

The current candidate is `0.1.0-rc.3`. It retains the published RC.2 runtime
implementation and adds the sustained production-scope gate described below.
Package versions, exact internal dependency
pins, and the release tag must agree. Both workflows enable zrelease's lockstep
policy; a stable-looking tag over RC packages is rejected. This cycle prepares
and qualifies an RC; final `0.1.0` publication remains a separate action.

## Production support contract

The release standard for `0.1.0` is **production ready within the documented
supported workloads and platforms**, with a deliberately limited feature set.
Supported applications use `vthread` or its `vthreads` alias on Linux x86_64 or
macOS ARM64 with Rust 1.96+ and unwinding panics. Internal support crates are
implementation dependencies, without a direct-use compatibility commitment.

Structured ownership, carrier affinity, bounded admission and services,
cooperative cancellation, and controlled shutdown are supported contracts.
Correctness failures within those contracts are bugs to fix. Public API and
contract changes remain compatible throughout `0.1.x`; breaking changes require
`0.2`. Standard-library blocking, arbitrary native faults, stack overflow, and
non-cooperative work retain the boundaries documented in [Security](SECURITY.md).

Feature completeness is not a release gate. Native correctness checks, the
application load/failure matrix, sustained qualification, and exact archive and
consumer verification are gates. A passing RC qualifies its own version and
source; final-version manifests and archives must pass the release gates again.

## Inbox progress repairs

The notification weaknesses discussed in the older release notes have concrete
repairs in the current runtime. Active carriers treat published inbox depth as
work to receive, independently of a delayed notification. A later publisher wakes
an already parked carrier, and waiter registration rechecks the queue under its
mutex before sleeping. These changes are recorded in
[`f92e5de`](https://github.com/zsumz/vthread/commit/f92e5dec76ae52340dd2086649538a1bfa1ef723)
and [`1ac097c`](https://github.com/zsumz/vthread/commit/1ac097c3039c9a167e7504f74edcd652921345fc).

The canonical gate exercises paused notifiers, active and parked carriers,
registration races, small queues with multiple producers, and a production-shaped
4,096-task refill. Boundary regressions demonstrated failures before the repairs;
the sustained gate exercises repeated lifetimes and reclamation on the repaired
runtime. The old pre-repair warning does not describe today's notification protocol.

## Sustained qualification

The full **Release** workflow, including `publish: false`, requires
[sustained runtime](.github/workflows/sustained.yml) before creating a release
plan. The compact automatic branch rehearsal remains a faster package check.

| Platform | Carriers | Mixed worker batch | Required uninterrupted duration |
| --- | --- | --- | --- |
| Linux x86_64 | 1 and 4, separate processes | 4,096 tasks | One hour per process |
| macOS ARM64 | 1 and 4, separate processes | 4,096 tasks | One hour per process |

Each default-feature optimized process retains one runtime across batches and
performs payload-checked TCP and bounded-channel exchanges, contended mutex
handoffs, semaphore admission, timers, native blocking jobs, carrier-affinity
checks, and cancellation races. Every batch checks service drain; final shutdown
checks active tasks, pending wakes, readiness registrations and native work.
The supervisor requires at least one million completed task lifetimes per
process, exact spawn/completion and park/wake accounting, exact stack acquisition
accounting, and the expected mutex updates. A timeout, crash, or restart fails.

RSS and open descriptors are sampled every ten seconds. After ten minutes of
warmup, the baseline is the next ten-minute median. The final ten-minute median
and every warmed sample must stay within the larger of 32 MiB or 20% of baseline
RSS, and within eight descriptors of baseline. Missing measurements fail the
gate. These are declared resource-growth checks for this workload, not a fixed
memory-per-task or universal throughput promise. The receipt records source,
lockfile and binary hashes, host, workload counts, samples, and gate results.

The separate 22-case application matrix checks concurrency 1/16/64/256 with one
and four carriers, fixed arrivals at 2,000/second, overload rejection, deadline
recovery, and shutdown while clients are blocked. These configurations define
the tested scope; they are not maximum supported capacity or latency guarantees.

## Practice a release

Push a `release/**` branch, or run **Actions → Rehearse** once the workflow is
on the default branch. Its compact graph shows **Package workspace → Attest →
Rehearse**, with individual crates in the logs and receipts. Branch rehearsals qualify that branch's
commit; tag rehearsals require the commit to be reachable from `main`.

The workflow first runs the canonical checks, the full application matrix, and
native-stack checks on Linux x86_64 and macOS ARM64. zrelease then processes:

1. `vthread-stack`
2. `vthread-sync-core`
3. `vthread`
4. `vthreads`

The lab, benchmark, and reference packages remain unpublished. Each selected crate
is tested from source and from its packaged archive, then built, tested, and run
in a fresh consumer against staged archives. The runtime and alias consumers run
the README task example and assert its result. Rehearsal also simulates a lost
upload acknowledgement and checks that retry does not publish twice. It never
publishes to crates.io and needs no release approval.

The sync-core archive excludes the two workspace model tests that import
`vthread` source. Canonical CI still runs both models; the archive retains
sync-core's own unit and mailbox model tests.

Keep the run's release plan, candidate archives, attestations, rehearsal reports,
and delivery receipts. zrelease retains these artifacts for 90 days. Its package
and consumer jobs currently run on Linux; the repository's runtime checks still
cover both supported platforms.

## Publish

All four crate names exist on crates.io. On 2026-09-11, `vthread-sync-core`
was bootstrapped as `0.1.0-rc.1` using the exact qualified archive from
[rehearsal 34550912875](https://github.com/zsumz/vthread/actions/runs/34550912875)
at source `5953345cd35579da620f926423f6faa3e17b9f58`. Its archive SHA-256 is
`3abcfe078199a8882ca7ae26f58eabd8bf563cc7d401ffbf07fc7d441f02e19e`.
The attestation, registry bytes, and fresh registry-only consumer build, test,
and run were verified. That one-time API-token bootstrap is complete; do not
repeat it or put a long-lived token into the reusable release jobs.

GitHub environments and all four crates.io Trusted Publishers were configured
and verified on 2026-09-11:

- Each publisher binds repository `zsumz/vthread`, workflow `release.yml`, and
  environment `crates.io`.
- Both environments allow only `v*` tags and disable admin bypass.
- `release` requires zsumz review with self-review allowed.
- `crates.io` has no second reviewer gate.

Confirm these settings before a live dispatch. zrelease requests one approval
for the complete plan. It checks every selected crate name before approval and
again before requesting upload credentials; a missing crate or registry error
stops the release before any upload. Existence alone does not establish ownership
or publisher permissions. A newly added crate needs its own qualified bootstrap
and Trusted Publisher before joining a live workspace release.

Keep every workspace package and internal dependency pin on the shared version,
update the changelog and installation examples, and merge the qualified source
to `main`. Create its PGP-signed `v<version>` tag and dispatch **Release** on that
tag with `publish: true`. Review the exact plan and approve the `release`
environment once. zrelease publishes and verifies each crate before proceeding
to the next. Publishing remains an explicit maintainer action.

Rerun failed jobs in the same run, retaining its candidate artifacts. zrelease
checks registry checksums before retrying and never automatically yanks crates.

## RC2 qualification and publication

The signed reconciliation commit joined public `main` and the reviewed
RC history while retaining the RC tree exactly. Both previous tips are preserved
under local `backup/*-before-release-*-20260911` refs. The ancestry guard remains
enabled. The reviewed integration was merged onto `main`; signed tag
`v0.1.0-rc.2` identifies `793e675c2c3ddc13940607ab4ec3342e209d0568`.
Do not move that tag to refresh release notes or pipeline dependencies.

The full **Release** rehearsal with `publish: false` succeeded in
[run 34609665912](https://github.com/zsumz/vthread/actions/runs/34609665912).
All four candidate archives, attestations, loopback recovery reports, and final
delivery receipts were verified against the exact tag and pinned pipeline.
This completed the full rehearsal requirement; another unchanged rehearsal is
not needed before a live RC dispatch. Rehearsal alone does not prove approval,
OIDC exchange, a real registry upload, or post-publication registry-only consumers.

The subsequent live **Release** completed on 2026-09-11 in
[run 34615332556](https://github.com/zsumz/vthread/actions/runs/34615332556),
publishing all four crates at `0.1.0-rc.2`. The single plan approval, all four
OIDC exchanges, registry uploads, and registry-only consumer builds, tests, and
runs succeeded. Every final delivery receipt is `consumer-verified`.
The approved plan, candidate attestations, downloaded registry bytes, consumer
lockfiles, and receipts were independently verified against the signed tag and
pinned pipeline. This establishes the live RC path; no final `0.1.0` was created.

RC-to-final automation should prepare a version-change PR from a verified RC
receipt, updating manifests, exact dependency pins, lockfiles and release docs.
That new commit needs its own qualification and approval. Simply retagging an RC
cannot change its packaged version. Final promotion remains a future design item.

## Update zrelease

From a pushed zrelease checkout with Node.js 24 and Rust 1.96.1, generate a new
caller into a temporary file:

```sh
node dist/install.mjs --sha "$(git rev-parse HEAD)" \
  --source /path/to/vthread --workspace --lockstep --toolchain 1.96.1 \
  --out /path/to/vthread/target/release.generated.yml
node dist/install.mjs --sha "$(git rev-parse HEAD)" \
  --source /path/to/vthread --workspace --lockstep --rehearsal --toolchain 1.96.1 \
  --out /path/to/vthread/target/rehearse.generated.yml
```

Regenerate when the workspace dependency graph changes. Preserve the caller's
canonical, native-stack and sustained prerequisites, automatic branch rehearsal, explicit
publish condition, `main` requirement for publication, and the consumer smoke
inputs and lockstep policy in both workflows. Run `actionlint` and `zcheck run check` before committing the update.

## Runtime coverage and limits

vthread requires Rust 1.96 or newer, Linux x86_64 or macOS ARM64, and unwinding
panics. Standard-library blocking calls occupy the carrier; they are not
transparently virtualized. Started tasks retain carrier affinity. Scopes own
children, resource counts are bounded, and cancellation is cooperative.

`zcheck run check` covers native debug and release tests, all-feature tests,
documentation and compile-fail examples, source and architecture policy,
panic-isolation regressions, application smoke tests, and standalone benchmark
checks. Hosted application qualification adds eight closed-loop loads, eight
fixed-arrival cases, and six failure rounds. Native-stack CI preserves source
and binary identities for debug and release on both targets.

The RC signoff is the runtime baseline; each release run records fresh evidence
for its own source commit and archives. Historical closeout records remain in
Git history and the archived candidate evidence. Automation does not establish
new scale or performance claims. The sustained matrix above adds cross-platform
lifetime and resource-growth coverage. Alternate-stack sanitizer hooks, populations
above the tested scope, absolute memory footprint, controlled loaded-tail targets,
and controlled-host idle CPU remain unqualified. Local timing is
observational; `zcheck run perf-cancellation-history` is a separate timing guard.

The eventual `0.1.0` and subsequent `0.1.x` releases preserve public API
compatibility within `0.1`; breaking API or contract changes move to `0.2`.
The exact dependency set includes `zio = "=0.0.1-dev.1"`; a normal vthread version
does not imply a stable-version contract for every dependency.
