# Releases

The `0.1.0-rc.1` runtime was signed off at
[`c4b2380`](https://github.com/zsumz/vthread/commit/c4b2380138b9f7f7384b2da9cb4ee9803e229588).
Release automation uses [zrelease](https://github.com/zsumz/zrelease), pinned to
`7cde759537b040d60e577f282d779b48c51ee9fe` in
[the Release workflow](.github/workflows/release.yml).

## Practice a release

Push a `release/**` branch, or run **Actions → Release** with `publish: false` once
the workflow is on the default branch. Branch rehearsals qualify that branch's
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

Create a GitHub `release` environment with required reviewers; allow self-review
if the maintainer starts the release. Create a `crates.io` environment restricted
to release tags, with no required reviewers. zrelease requests one approval for
the complete plan.

Keep every workspace package and internal dependency pin on the shared version,
update the changelog and installation examples, and merge the qualified source
to `main`. Create its PGP-signed `v<version>` tag and dispatch **Release** on that
tag with `publish: true`. Review the exact plan and approve the `release`
environment once. zrelease publishes and verifies each crate before proceeding
to the next. Publishing remains an explicit maintainer action.

Rerun failed jobs in the same run, retaining its candidate artifacts. zrelease
checks registry checksums before retrying and never automatically yanks crates.

## Update zrelease

From a pushed zrelease checkout with Node.js 24 and Rust 1.96.1, generate a new
caller into a temporary file:

```sh
node dist/install.mjs --sha "$(git rev-parse HEAD)" \
  --source /path/to/vthread --workspace --toolchain 1.96.1 \
  --out /path/to/vthread/target/release.generated.yml
```

Regenerate when the workspace dependency graph changes. Preserve the caller's
canonical and native-stack prerequisites, automatic branch rehearsal, explicit
publish condition, `main` requirement for publication, and both consumer smoke
inputs. Run `actionlint` and `zcheck run check` before committing the update.

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
