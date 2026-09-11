# Releases

The `0.1.0-rc.1` runtime was signed off at
[`c4b2380`](https://github.com/zsumz/vthread/commit/c4b2380138b9f7f7384b2da9cb4ee9803e229588).
Release automation uses [zrelease](https://github.com/zsumz/zrelease), pinned to
`1993b45bfae19a00e61a9138565b79e029c2d7f4` in
[the Rehearse workflow](.github/workflows/rehearse.yml) and
[the Release workflow](.github/workflows/release.yml).

The next candidate is `0.1.0-rc.2`. Package versions, exact internal dependency
pins, and the release tag must agree. Both workflows enable zrelease's lockstep
policy; a stable-looking tag over RC packages is rejected. No final release is
being prepared in this cycle.

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

Before the first real publication, configure crates.io Trusted Publishing for
all four crates with repository `zsumz/vthread`, workflow `release.yml`, and
environment `crates.io`. Each crate must already exist on crates.io.

The release review on 2026-09-11 confirmed that `vthread-sync-core` does not exist
on crates.io. The other three crates exist at `0.0.2-rc.1`. zrelease now checks
every selected crate name before approval and again before requesting upload
credentials; a missing crate or registry error stops the release before any
upload. This check does not establish ownership or publisher permissions.

Bootstrap sync-core separately as `0.1.0-rc.1`, using its exact qualified archive
from [rehearsal 34550912875](https://github.com/zsumz/vthread/actions/runs/34550912875)
at source `5953345cd35579da620f926423f6faa3e17b9f58`. Preserve and verify the
workspace attestation, candidate digest, archive digest and upload metadata from
that run. Its first publication requires an API token; do not put a long-lived
token into the reusable release jobs. Then register its Trusted Publisher.
The complete workspace will use `0.1.0-rc.2`, so bootstrap bytes cannot conflict
with a newly generated archive at the same version.

Create a GitHub `release` environment with required reviewers; allow self-review
if the maintainer starts the release. Create a `crates.io` environment restricted
to release tags, with no required reviewers. zrelease requests one approval for
the complete plan.

The review found neither environment configured. Confirm both environment rules
and all four crates.io registrations before enabling publication. Creating an
environment alone does not configure Trusted Publishing.

Keep every workspace package and internal dependency pin on the shared version,
update the changelog and installation examples, and merge the qualified source
to `main`. Create its PGP-signed `v<version>` tag and dispatch **Release** on that
tag with `publish: true`. Review the exact plan and approve the `release`
environment once. zrelease publishes and verifies each crate before proceeding
to the next. Publishing remains an explicit maintainer action.

Rerun failed jobs in the same run, retaining its candidate artifacts. zrelease
checks registry checksums before retrying and never automatically yanks crates.

## Qualify this RC integration

The signed reconciliation commit joins current public `main` and the reviewed
RC history while retaining the RC tree exactly. Both previous tips are preserved
under local `backup/*-before-release-*-20260911` refs. The ancestry guard remains
enabled. Merge the reviewed integration onto `main` before tagging it.

Create a signed `v0.1.0-rc.2` tag on the resulting qualified commit and dispatch
the full **Release** workflow with `publish: false`. Keep its per-crate candidates,
attestations and delivery receipts. This exercises the full release graph and
bookkeeping; compact Rehearse success is supplementary evidence. It still does
not prove approval, OIDC exchange, a real registry upload or registry-only
consumers. A controlled live RC must establish those before any final release.

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
canonical and native-stack prerequisites, automatic branch rehearsal, explicit
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
new scale or performance claims. Alternate-stack sanitizer hooks, larger
simultaneous populations, cross-platform sustained runs, memory footprint,
loaded tails, and controlled-host idle CPU remain unqualified. Local timing is
observational; `zcheck run perf-cancellation-history` is a separate timing guard.

The eventual `0.1.0` and subsequent `0.1.x` releases preserve public API
compatibility within `0.1`; breaking API or contract changes move to `0.2`.
The exact dependency set includes `zio = "=0.0.1-dev.1"`; a normal vthread version
does not imply a stable-version contract for every dependency.
